// Every scene of the Manuscript, or of one story in a collection, laid out
// left to right in story order, once in each row whose character or place it
// names. It stands over the binder and the tabs, and Escape goes back to them.

import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { DragEvent, MouseEvent as ReactMouseEvent } from "react";
import { createPortal } from "react-dom";
import { invoke } from "@tauri-apps/api/core";
import type { DocumentSummary } from "./bindings/DocumentSummary";
import type { Marker } from "./bindings/Marker";
import type { Timeline as Laid } from "./bindings/Timeline";
import type { TimelineView } from "./bindings/TimelineView";
import { useCorpus } from "./corpus";
import { failure } from "./errors";
import Icon from "./Icon";
import { counted, previewed } from "./cards";
import { list, parse } from "./frontmatter";
import { inward } from "./lib/overflow";
import Menu, {
	MenuItem,
	PointerMenu,
	useDismissal,
	useRightClick,
	type Point,
} from "./Menu";
import NameField from "./NameField";
import { useFormat } from "./projectFormat";
import {
	dropAt,
	gapAt,
	markerAt,
	rowsOf,
	stretchesOf,
	subjectsOf,
} from "./timeline";
import type { FolderRef } from "./tree";

type Props = {
	root: string;
	folder: FolderRef;
	changed: number;
	live: Map<string, string>;
	onBack: () => void;
	onOpen: (scene: DocumentSummary) => void;
};

/** A scene's card, open under the block that was clicked. */
type Opened = { scene: DocumentSummary; at: Point; from: HTMLElement };

type Read =
	| { kind: "reading" }
	| { kind: "ready"; laid: Laid }
	| { kind: "failed"; message: string };

/** What is being dragged: a scene's block or a marker's band. */
type Dragged = { kind: "scene" | "marker"; id: string };

/** A command that would carry out the drop, and its arguments past `root`. */
type Call = [string, Record<string, unknown>];

/** The name field, open on a gap for a new marker or on a marker. */
type Naming = { kind: "new"; gap: number } | { kind: "rename"; marker: Marker };

const FRESH: TimelineView = {
	rows: "characters",
	hiddenCharacters: [],
	hiddenLocations: [],
};

/** What each side of the switch reads from and says. */
const SIDES = {
	characters: {
		section: "Characters",
		label: "Characters",
		none: "No one",
		hidden: "hiddenCharacters",
	},
	locations: {
		section: "Locations",
		label: "Locations",
		none: "Nowhere",
		hidden: "hiddenLocations",
	},
} as const;

export default function Timeline({
	root,
	folder,
	changed,
	live,
	onBack,
	onOpen,
}: Props) {
	const [read, setRead] = useState<Read>({ kind: "reading" });
	const [view, setView] = useState<TimelineView>(FRESH);
	const screenplay = useFormat() === "screenplay";
	const [reread, setReread] = useState(0);
	const [dragging, setDragging] = useState<Dragged | null>(null);
	const [over, setOver] = useState<{ gap: number; left: number } | null>(
		null,
	);
	const [trouble, setTrouble] = useState<string | null>(null);
	const [naming, setNaming] = useState<Naming | null>(null);
	const [nameBusy, setNameBusy] = useState(false);
	const [nameError, setNameError] = useState<string | null>(null);
	const [menuFor, setMenuFor] = useState<Marker | null>(null);
	const [card, setCard] = useState<Opened | null>(null);
	const pointer = useRightClick();
	const grid = useRef<HTMLDivElement>(null);

	useEffect(() => {
		let gone = false;
		invoke<Laid>("timeline", { root, folder: folder.id })
			.then((laid) => {
				if (!gone) {
					setRead({ kind: "ready", laid });
				}
			})
			.catch((error: unknown) => {
				if (!gone) {
					setRead({
						kind: "failed",
						message: failure(error).message,
					});
				}
			});
		return () => {
			gone = true;
		};
	}, [root, folder.id, changed, reread]);

	// A store that cannot be read leaves the timeline as it first opens.
	useEffect(() => {
		let gone = false;
		invoke<TimelineView>("read_timeline_view", { root })
			.then((stored) => {
				if (!gone) {
					setView(stored);
				}
			})
			.catch(() => undefined);
		return () => {
			gone = true;
		};
	}, [root]);

	function change(next: TimelineView) {
		setView(next);
		void invoke("write_timeline_view", { root, view: next }).catch(
			() => undefined,
		);
	}

	// On the window, so a menu open over the timeline, listening on the
	// document, takes its own Escape first.
	useEffect(() => {
		function escape(event: KeyboardEvent) {
			if (event.key === "Escape" && !event.defaultPrevented) {
				onBack();
			}
		}
		window.addEventListener("keydown", escape);
		return () => window.removeEventListener("keydown", escape);
	}, [onBack]);

	const { corpus, documents } = useCorpus({
		root,
		moved: changed,
		live,
		wanted: true,
	});

	const side = SIDES[view.rows];
	const hidden = view[side.hidden];
	const scenes = read.kind === "ready" ? read.laid.scenes : [];
	const markers = read.kind === "ready" ? read.laid.markers : [];
	const order = useMemo(() => scenes.map((scene) => scene.id), [scenes]);
	const stretches = useMemo(
		() => stretchesOf(markers, order),
		[markers, order],
	);
	const subjects = useMemo(
		() => subjectsOf(documents, side.section),
		[documents, side.section],
	);
	const rows = useMemo(
		() =>
			rowsOf(order, documents, side.section, screenplay, new Set(hidden)),
		[order, documents, side.section, screenplay, hidden],
	);

	/**
	 * The columns as drawn, measured off the column numbers: where the first
	 * starts, how far apart they are, and how far into the grid a gap sits.
	 */
	function measure() {
		const box = grid.current;
		const first = box?.querySelector<HTMLElement>(".timeline__tick");
		if (!box || !first) {
			return null;
		}
		const space = parseFloat(getComputedStyle(box).columnGap) || 0;
		const pitch = first.offsetWidth + space;
		const start = first.getBoundingClientRect().left;
		const offset = start - box.getBoundingClientRect().left - space / 2;
		return { start, pitch, line: (gap: number) => offset + gap * pitch };
	}

	/** The gap the drag would come down in, and the command that puts it there. */
	function aimed(event: DragEvent) {
		const at = measure();
		if (at === null || dragging === null || read.kind !== "ready") {
			return null;
		}
		const x = event.clientX - at.start;
		const landing = ((): { gap: number; call: Call } | null => {
			if (dragging.kind === "scene") {
				const drop = dropAt(
					x,
					at.pitch,
					order,
					dragging.id,
					read.laid.placed,
				);
				return drop === null
					? null
					: {
							gap: drop.gap,
							call: [
								"place_scene",
								{
									id: dragging.id,
									place: drop.place,
									before: drop.before,
								},
							],
						};
			}
			const gap = gapAt(x, at.pitch, order.length);
			const before = markerAt(gap, order, stretches);
			return before === null
				? null
				: { gap, call: ["move_marker", { id: dragging.id, before }] };
		})();
		return landing === null
			? null
			: { ...landing, left: at.line(landing.gap) };
	}

	function run([command, args]: Call) {
		invoke(command, { root, ...args })
			.then(() => setReread((count) => count + 1))
			.catch((error: unknown) => setTrouble(failure(error).message));
	}

	function startDrag(event: DragEvent, what: Dragged) {
		event.dataTransfer.effectAllowed = "move";
		// A drag with nothing on the transfer does not start in some browsers.
		event.dataTransfer.setData("text/plain", what.id);
		setTrouble(null);
		setDragging(what);
	}

	/** Opens the name field on the gap nearest a click on the column numbers. */
	function aimName(event: ReactMouseEvent) {
		const at = measure();
		if (at === null) {
			return;
		}
		const gap = gapAt(event.clientX - at.start, at.pitch, order.length);
		if (markerAt(gap, order, stretches) !== null) {
			setNameError(null);
			setNaming({ kind: "new", gap });
		}
	}

	function submitName(name: string) {
		if (naming === null) {
			return;
		}
		setNameBusy(true);
		const saved =
			naming.kind === "new"
				? invoke("add_marker", {
						root,
						name,
						before: order[naming.gap],
					})
				: invoke("rename_marker", { root, id: naming.marker.id, name });
		saved
			.then(() => {
				setNaming(null);
				setReread((count) => count + 1);
			})
			.catch((error: unknown) => setNameError(failure(error).message))
			.finally(() => setNameBusy(false));
	}

	function nameField(initial: string) {
		return (
			<NameField
				label="Marker name"
				placeholder="Three years later"
				initial={initial}
				busy={nameBusy}
				working="Saving…"
				error={nameError}
				onSubmit={submitName}
				onCancel={() => setNaming(null)}
			/>
		);
	}

	function stop() {
		setDragging(null);
		setOver(null);
	}

	function hover(event: DragEvent) {
		const at = aimed(event);
		if (at === null) {
			setOver(null);
			return;
		}
		// Only a prevented dragover makes the grid a drop target.
		event.preventDefault();
		event.dataTransfer.dropEffect = "move";
		setOver((current) =>
			current?.gap === at.gap ? current : { gap: at.gap, left: at.left },
		);
	}

	function drop(event: DragEvent) {
		event.preventDefault();
		const at = aimed(event);
		stop();
		if (at !== null) {
			run(at.call);
		}
	}

	function toggle(id: string) {
		change({
			...view,
			[side.hidden]: hidden.includes(id)
				? hidden.filter((each) => each !== id)
				: [...hidden, id],
		});
	}

	return (
		<section className="timeline" aria-label={`Timeline of ${folder.name}`}>
			<header className="timeline__head">
				<div>
					{trouble === null ? (
						<p className="timeline__of">Timeline</p>
					) : (
						<p
							className="timeline__of timeline__trouble"
							role="alert"
						>
							{trouble}
						</p>
					)}
					<h1 className="timeline__title">{folder.name}</h1>
				</div>
				<div className="timeline__controls">
					<div
						className="versions__scope timeline__switch"
						role="group"
						aria-label="Rows"
					>
						{(["characters", "locations"] as const).map((rows) => (
							<button
								key={rows}
								type="button"
								className="versions__scope-button"
								aria-pressed={view.rows === rows}
								onClick={() => change({ ...view, rows })}
							>
								{SIDES[rows].label}
							</button>
						))}
					</div>
					<Menu
						label={`Which ${side.label.toLowerCase()} show`}
						text="Show"
						icon="chevron-down"
						trailing
						wrapper="timeline__menu"
						className="timeline__show"
						disabled={subjects.length === 0}
						title={
							subjects.length === 0
								? `This project has no ${side.label.toLowerCase()} pages`
								: undefined
						}
					>
						{() =>
							subjects.map((subject) => (
								<MenuItem
									key={subject.id}
									ticks
									checked={!hidden.includes(subject.id)}
									onSelect={() => toggle(subject.id)}
								>
									<span className="menu__tick">
										{!hidden.includes(subject.id) && (
											<Icon name="check" />
										)}
									</span>
									{subject.title}
								</MenuItem>
							))
						}
					</Menu>
				</div>
			</header>
			{read.kind === "failed" ? (
				<p className="timeline__note" role="alert">
					{read.message}
				</p>
			) : corpus.kind === "failed" ? (
				<p className="timeline__note" role="alert">
					{corpus.message}
				</p>
			) : read.kind === "reading" || corpus.kind !== "ready" ? (
				<p className="timeline__note">Reading the scenes…</p>
			) : scenes.length === 0 ? (
				<p className="timeline__note">No scenes here yet.</p>
			) : (
				<div className="timeline__scroll">
					<div
						ref={grid}
						className="timeline__grid"
						style={{
							gridTemplateColumns: `var(--timeline-label) repeat(${scenes.length}, var(--timeline-block))`,
						}}
						onDragOver={hover}
						onDragLeave={(event) => {
							if (
								!event.currentTarget.contains(
									event.relatedTarget as Node | null,
								)
							) {
								setOver(null);
							}
						}}
						onDrop={drop}
					>
						{over !== null && (
							<span
								className="timeline__drop"
								style={{ left: over.left }}
							/>
						)}
						<span className="timeline__corner" />
						<div
							className="timeline__strip"
							style={{
								gridTemplateColumns: `repeat(${scenes.length}, var(--timeline-block))`,
							}}
						>
							{stretches.map(({ marker, from, to }) => {
								const span = `${from + 1} / ${to + 1}`;
								return naming?.kind === "rename" &&
									naming.marker.id === marker.id ? (
									<div
										key={marker.id}
										className="timeline__naming"
										style={{ gridColumn: span }}
									>
										{nameField(marker.name)}
									</div>
								) : (
									<span
										key={marker.id}
										className="timeline__marker"
										style={{ gridColumn: span }}
										title={marker.name}
										tabIndex={0}
										draggable
										data-dragging={
											dragging?.id === marker.id
												? ""
												: undefined
										}
										onDragStart={(event) =>
											startDrag(event, {
												kind: "marker",
												id: marker.id,
											})
										}
										onDragEnd={stop}
										onContextMenu={(event) => {
											setMenuFor(marker);
											pointer.open(event);
										}}
									>
										{marker.name}
									</span>
								);
							})}
							{naming?.kind === "new" && (
								<div
									className="timeline__naming"
									style={{
										gridColumn: `${naming.gap + 1} / span ${Math.min(2, scenes.length - naming.gap)}`,
									}}
								>
									{nameField("")}
								</div>
							)}
						</div>
						<span className="timeline__corner" />
						{scenes.map((scene, at) => (
							<span
								key={scene.id}
								className="timeline__tick"
								title={scene.title}
								onClick={aimName}
							>
								{at + 1}
							</span>
						))}
						{rows.map((row) => (
							<div
								key={row.subject?.id ?? ""}
								className="timeline__row"
							>
								<span className="timeline__label">
									{row.subject?.title ?? side.none}
								</span>
								{scenes.map((scene) =>
									row.scenes.has(scene.id) ? (
										<button
											type="button"
											key={scene.id}
											className="timeline__block"
											title={scene.title}
											onClick={(event) => {
												const box =
													event.currentTarget.getBoundingClientRect();
												setCard({
													scene,
													at: {
														x: box.left,
														y: box.bottom + 4,
													},
													from: event.currentTarget,
												});
											}}
											draggable
											data-dragging={
												dragging?.id === scene.id
													? ""
													: undefined
											}
											onDragStart={(event) =>
												startDrag(event, {
													kind: "scene",
													id: scene.id,
												})
											}
											onDragEnd={stop}
										>
											{scene.title}
										</button>
									) : (
										<span key={scene.id} />
									),
								)}
							</div>
						))}
					</div>
				</div>
			)}
			<PointerMenu
				at={pointer.at}
				label={`Marker ${menuFor?.name ?? ""}`}
				onClose={pointer.close}
			>
				{(close) =>
					menuFor !== null && (
						<>
							<MenuItem
								onSelect={() => {
									close();
									setNameError(null);
									setNaming({
										kind: "rename",
										marker: menuFor,
									});
								}}
							>
								Rename
							</MenuItem>
							<MenuItem
								danger
								onSelect={() => {
									close();
									run(["delete_marker", { id: menuFor.id }]);
								}}
							>
								Delete
							</MenuItem>
						</>
					)
				}
			</PointerMenu>
			{card !== null && (
				<SceneCard
					key={card.scene.id}
					opened={card}
					onOpen={() => onOpen(card.scene)}
					onClose={() => setCard(null)}
				/>
			)}
		</section>
	);
}

/**
 * What the overview's card says about a scene, over the timeline: its synopsis
 * or opening, its tags and its count, with Open to go to it.
 */
function SceneCard({
	opened: { scene, at, from },
	onOpen,
	onClose,
}: {
	opened: Opened;
	onOpen: () => void;
	onClose: () => void;
}) {
	const box = useRef<HTMLDivElement>(null);
	const tags = list(parse(scene.front), "tags");

	// Focus goes back to the block, unless a click away already put it elsewhere.
	function close() {
		if (box.current?.contains(document.activeElement)) {
			from.focus();
		}
		onClose();
	}

	useDismissal(true, box, close);

	// Placed before it is painted, so it never shows past the window's edge.
	useLayoutEffect(() => {
		const card = box.current;
		if (card === null) {
			return;
		}
		const spot = inward(
			at,
			{ width: card.offsetWidth, height: card.offsetHeight },
			{ width: window.innerWidth, height: window.innerHeight },
		);
		card.style.left = `${spot.x}px`;
		card.style.top = `${spot.y}px`;
	}, [at]);

	// On the body, as a right-click menu is, so nothing around the timeline
	// becomes what `fixed` is measured against.
	return createPortal(
		<div
			ref={box}
			className="timeline__card"
			role="dialog"
			aria-label={scene.title}
		>
			<span className="card__title">{scene.title}</span>
			<span className="card__excerpt">
				{previewed(scene.front, scene.excerpt)}
			</span>
			{tags.length > 0 && (
				<ul className="timeline__tags" aria-label="Tags">
					{tags.map((tag) => (
						<li key={tag} className="timeline__tag">
							{tag}
						</li>
					))}
				</ul>
			)}
			<div className="timeline__card-foot">
				<span className="card__meta">
					{counted(scene.words, scene.target, scene.pages)}
				</span>
				<button
					type="button"
					className="timeline__open"
					autoFocus
					onClick={onOpen}
				>
					Open
				</button>
			</div>
		</div>,
		document.body,
	);
}
