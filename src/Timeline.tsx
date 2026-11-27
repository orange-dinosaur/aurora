// Every scene of the Manuscript, or of one story in a collection, laid out
// left to right in story order, once in each row whose character or place it
// names. It stands over the binder and the tabs, and Escape goes back to them.

import { useEffect, useMemo, useRef, useState } from "react";
import type { DragEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Timeline as Laid } from "./bindings/Timeline";
import type { TimelineView } from "./bindings/TimelineView";
import { useCorpus } from "./corpus";
import { failure } from "./errors";
import Icon from "./Icon";
import Menu, { MenuItem } from "./Menu";
import { useFormat } from "./projectFormat";
import { dropAt, rowsOf, subjectsOf, type Drop } from "./timeline";
import type { FolderRef } from "./tree";

type Props = {
	root: string;
	folder: FolderRef;
	changed: number;
	live: Map<string, string>;
	onBack: () => void;
};

type Read =
	| { kind: "reading" }
	| { kind: "ready"; laid: Laid }
	| { kind: "failed"; message: string };

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
}: Props) {
	const [read, setRead] = useState<Read>({ kind: "reading" });
	const [view, setView] = useState<TimelineView>(FRESH);
	const screenplay = useFormat() === "screenplay";
	const [placed, setPlaced] = useState(0);
	const [dragging, setDragging] = useState<string | null>(null);
	const [over, setOver] = useState<{ drop: Drop; left: number } | null>(null);
	const [trouble, setTrouble] = useState<string | null>(null);
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
	}, [root, folder.id, changed, placed]);

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
	const subjects = useMemo(
		() => subjectsOf(documents, side.section),
		[documents, side.section],
	);
	const rows = useMemo(
		() =>
			rowsOf(
				scenes.map((scene) => scene.id),
				documents,
				side.section,
				screenplay,
				new Set(hidden),
			),
		[scenes, documents, side.section, screenplay, hidden],
	);

	/**
	 * Where the dragged scene would land under the cursor, and how far into
	 * the grid to draw the line, measured off the column numbers.
	 */
	function aimed(event: DragEvent) {
		const box = grid.current;
		const first = box?.querySelector<HTMLElement>(".timeline__tick");
		if (
			read.kind !== "ready" ||
			dragging === null ||
			box === null ||
			!first
		) {
			return null;
		}
		const space = parseFloat(getComputedStyle(box).columnGap) || 0;
		const pitch = first.offsetWidth + space;
		const start = first.getBoundingClientRect().left;
		const drop = dropAt(
			event.clientX - start,
			pitch,
			scenes.map((scene) => scene.id),
			dragging,
			read.laid.placed,
		);
		return drop === null
			? null
			: {
					drop,
					left:
						start -
						box.getBoundingClientRect().left +
						drop.gap * pitch -
						space / 2,
				};
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
			current?.drop.gap === at.drop.gap ? current : at,
		);
	}

	function drop(event: DragEvent) {
		event.preventDefault();
		const at = aimed(event);
		const id = dragging;
		stop();
		if (at === null || id === null) {
			return;
		}
		invoke("place_scene", {
			root,
			id,
			place: at.drop.place,
			before: at.drop.before,
		})
			.then(() => setPlaced((count) => count + 1))
			.catch((error: unknown) => setTrouble(failure(error).message));
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
						{scenes.map((scene, at) => (
							<span
								key={scene.id}
								className="timeline__tick"
								title={scene.title}
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
										<span
											key={scene.id}
											className="timeline__block"
											title={scene.title}
											draggable
											data-dragging={
												dragging === scene.id
													? ""
													: undefined
											}
											onDragStart={(event) => {
												event.dataTransfer.effectAllowed =
													"move";
												// A drag with nothing on the transfer
												// does not start in some browsers.
												event.dataTransfer.setData(
													"text/plain",
													scene.id,
												);
												setTrouble(null);
												setDragging(scene.id);
											}}
											onDragEnd={stop}
										>
											{scene.title}
										</span>
									) : (
										<span key={scene.id} />
									),
								)}
							</div>
						))}
					</div>
				</div>
			)}
		</section>
	);
}
