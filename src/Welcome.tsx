import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { CSSProperties, MouseEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { homeDir } from "@tauri-apps/api/path";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import ThemeToggle from "./ThemeToggle";
import type { ShelfView } from "./bindings/ShelfView";
import type {
	Format,
	FormatLayout,
	OpenProject,
	RecentSummary,
	Start,
	StructureLayout,
	Theme,
} from "./types";
import { AccountChip } from "./Account";
import { useCoverImage } from "./Cover";
import Icon, { type IconName } from "./Icon";
import Menu, {
	DIALOG_FADE,
	MenuItem,
	useDismissal,
	useLingering,
} from "./Menu";
import { coverFor } from "./lib/covers";
import { settle, spots, type Spots } from "./lib/settle";
import { going, place } from "./recents";
import { when } from "./dates";
import { failure } from "./errors";
import NewProject, { COPY, describe, type FormatOption } from "./NewProject";
import { paged } from "./cards";
import {
	bookTitle,
	formatCounts,
	shelved,
	type ShelfFormat,
	type ShelfSort,
} from "./shelf";

/** How each format shows on the shelf: its chip, and its tag's icon and colour. */
const FORMATS: Record<Format, { plural: string; icon: IconName; hue: number }> =
	{
		novel: { plural: "Novels", icon: "book", hue: 1 },
		screenplay: { plural: "Screenplays", icon: "clapperboard", hue: 4 },
		"short-stories": { plural: "Short stories", icon: "copy", hue: 3 },
		"stage-play": { plural: "Stage plays", icon: "layers", hue: 2 },
	};

const SORTS: Record<ShelfSort, string> = {
	opened: "Last opened",
	title: "Title",
	length: "Length",
};

const SORT_ORDER = Object.keys(SORTS) as ShelfSort[];

// How big a project is. A project whose manifest could not be read has no
// count, rather than a made-up one.
function length(project: RecentSummary): string | null {
	return project.pages !== null
		? paged(project.pages)
		: project.words === null
			? null
			: `${project.words.toLocaleString()} words`;
}

function opened(project: RecentSummary): string | null {
	return project.lastOpened === null ? null : when(project.lastOpened);
}

function books(count: number): string {
	return count === 1 ? "1 book" : `${count.toLocaleString()} books`;
}

type Status =
	{ kind: "idle" } | { kind: "busy" } | { kind: "error"; message: string };

type Props = {
	notice: string | null;
	/** `created` when the project was made just now rather than reopened. */
	onOpened: (project: OpenProject, created?: boolean) => void;
	onLogin: () => void;
	onProfile: () => void;
	onSettings: (keyboard: boolean) => void;
	theme: Theme;
	onTheme: (theme: Theme) => void;
	shelfView: ShelfView;
	onShelfView: (view: ShelfView) => void;
	/** Where new projects go, or null while the writer has never been asked. */
	projectsFolder: string | null;
	onProjectsFolder: (folder: string) => void;
};

/** The format's icon and name, in its colour. */
function FormatTag({ format }: { format: Format }) {
	return (
		<span
			className="shelf__tag"
			style={
				{
					"--c": `var(--block-${FORMATS[format].hue})`,
				} as CSSProperties
			}
		>
			<Icon name={FORMATS[format].icon} />
			{COPY[format].name}
		</span>
	);
}

/** One project on the Shelf, as a cover or as a row of the list. */
function Book({
	project,
	view,
	busy,
	asking,
	onOpen,
	onShow,
	onForget,
	onAsk,
	onCancel,
	onDelete,
	home,
}: {
	project: RecentSummary;
	view: ShelfView;
	busy: boolean;
	/** For writing a book from elsewhere's folder with `~`. */
	home: string | null;
	/** Whether the book is asking if it should be deleted. */
	asking: boolean;
	onOpen: () => void;
	onShow: () => void;
	onForget: () => void;
	onAsk: () => void;
	onCancel: () => void;
	onDelete: () => void;
}) {
	const ask = useRef<HTMLDivElement>(null);
	useDismissal(asking, ask, onCancel);

	const { src } = useCoverImage(project.root, project.cover ?? "", 0);
	const unread = project.words === null;
	const title = bookTitle(project);

	/** The cover, with its words only where there is room for them. */
	function cover(small: boolean) {
		if (unread) {
			return (
				<span className="shelf__cover shelf__cover--unread">
					{!small && (
						<>
							<strong>{title}</strong>
							<span>Aurora can't read this project.</span>
						</>
					)}
				</span>
			);
		}
		// A wash of the title stands behind an image too, so the cover is
		// never blank while one loads or if it cannot be read.
		return (
			<span
				className={
					src === ""
						? "shelf__cover"
						: "shelf__cover shelf__cover--image"
				}
				style={{ background: coverFor(title) }}
			>
				{src !== "" && <img src={src} alt="" />}
				{!small && (
					<>
						<span className="shelf__title">{title}</span>
						<span className="shelf__author">{project.author}</span>
					</>
				)}
			</span>
		);
	}

	const question = asking && (
		<div ref={ask} className="shelf__ask" role="alertdialog">
			<p className="shelf__question">Delete {title}?</p>
			<p className="shelf__going">{going(project.documents)}</p>
			<div className="shelf__answers">
				{/* Focused rather than Delete, so Enter cannot lose work. */}
				<button
					type="button"
					className="shelf__cancel"
					autoFocus
					disabled={busy}
					onClick={onCancel}
				>
					Cancel
				</button>
				<button
					type="button"
					className="shelf__sure"
					disabled={busy}
					onClick={onDelete}
				>
					Delete project
				</button>
			</div>
		</div>
	);

	const actions = (
		<Menu
			label={`Actions for ${title}`}
			icon="more-horizontal"
			wrapper={view === "list" ? "shelf__row-menu" : undefined}
		>
			{(close) => (
				<>
					{/* What tells two books of the same title apart. */}
					<p className="menu__note shelf__path">{project.root}</p>

					<span className="menu__rule" />

					<MenuItem
						onSelect={() => {
							close();
							onShow();
						}}
					>
						Show in Files
					</MenuItem>
					<MenuItem
						onSelect={() => {
							close();
							onForget();
						}}
					>
						Remove from Aurora
						<span className="menu__hint">
							Leaves the files where they are
						</span>
					</MenuItem>

					<span className="menu__rule" />

					<MenuItem
						danger
						onSelect={() => {
							close();
							onAsk();
						}}
					>
						Delete project…
						<span className="menu__hint">
							Moves the whole folder to the trash
						</span>
					</MenuItem>
				</>
			)}
		</Menu>
	);

	const where = project.elsewhere ? place(project.root, home) : null;

	if (view === "list") {
		return (
			<li className="shelf__row" data-settle={project.root}>
				{cover(true)}
				{/* Its click reaches across the whole row. */}
				<button
					type="button"
					className="shelf__row-open"
					disabled={busy}
					onClick={onOpen}
				>
					<b>{title}</b>
					<span>
						{[
							unread
								? "Aurora can't read this project."
								: project.author,
							where,
						]
							.filter(Boolean)
							.join(" · ")}
					</span>
				</button>
				<span className="shelf__cell shelf__cell--format">
					{project.format !== null && (
						<FormatTag format={project.format} />
					)}
				</span>
				<span
					className="shelf__cell shelf__cell--left"
					title={project.leftOff ?? undefined}
				>
					{project.leftOff}
				</span>
				<span className="shelf__cell shelf__cell--number">
					{length(project)}
				</span>
				<span className="shelf__cell shelf__cell--number shelf__cell--date">
					{opened(project)}
				</span>
				{question}
				{actions}
			</li>
		);
	}

	return (
		<li className="shelf__book" data-settle={project.root}>
			{/* Still a button when it cannot be read: opening it is how the
			    writer finds out why, at the foot of the window. */}
			<button
				type="button"
				className="shelf__open"
				disabled={busy}
				onClick={onOpen}
			>
				{cover(false)}
				{project.format !== null && (
					<FormatTag format={project.format} />
				)}
				<span className="shelf__meta">
					{[length(project), opened(project)]
						.filter(Boolean)
						.join(" · ")}
				</span>
				{project.leftOff !== null && (
					<span
						className="shelf__meta shelf__left"
						title={project.leftOff}
					>
						{project.leftOff}
					</span>
				)}
				{where !== null && (
					<span className="shelf__meta shelf__place">
						<Icon name="link" />
						<span>{where}</span>
					</span>
				)}
			</button>
			{question}
			{actions}
		</li>
	);
}

export default function Welcome({
	notice,
	onOpened,
	onLogin,
	onProfile,
	onSettings,
	theme,
	onTheme,
	shelfView,
	onShelfView,
	projectsFolder,
	onProjectsFolder,
}: Props) {
	// Seeded once; the notice describes how this screen was reached.
	const [reason, setReason] = useState(notice);
	const [creating, setCreating] = useState(false);
	// The keyboard reports a click with no count, and skips the motion.
	const [instant, setInstant] = useState(false);
	const drawn = useLingering(creating, DIALOG_FADE);
	const [formats, setFormats] = useState<FormatOption[]>([]);
	const [structures, setStructures] = useState<StructureLayout[]>([]);
	// Null until the list arrives. Counting every project's words takes a
	// moment, and the empty Shelf must not flash up in front of a full one.
	const [recents, setRecents] = useState<RecentSummary[] | null>(null);
	// The project whose book is asking to be deleted. One at a time, since a
	// click anywhere else answers Cancel.
	const [asking, setAsking] = useState<string | null>(null);
	const [home, setHome] = useState<string | null>(null);
	const [format, setFormat] = useState<ShelfFormat>("all");
	const [search, setSearch] = useState("");
	const [sort, setSort] = useState<ShelfSort>("opened");

	// Without it the place line shows the whole path, which is still right.
	useEffect(() => {
		homeDir().then(setHome, () => {});
	}, []);
	const [status, setStatus] = useState<Status>({ kind: "idle" });

	const busy = status.kind === "busy";

	useEffect(() => {
		invoke<FormatLayout[]>("format_layouts")
			.then((layouts) => setFormats(layouts.map(describe)))
			.catch((error) =>
				setStatus({ kind: "error", message: failure(error).message }),
			);
		// Without the list a novel starts as a single scene, as it always has.
		invoke<StructureLayout[]>("structure_layouts")
			.then(setStructures)
			.catch(() => setStructures([]));
	}, []);

	// Again when the projects folder changes, since the shelf lists it.
	useEffect(() => {
		invoke<RecentSummary[]>("recent_projects")
			.then(setRecents)
			.catch(() => setRecents([]));
	}, [projectsFolder]);

	function startNew(event: MouseEvent) {
		setInstant(event.detail === 0);
		setCreating(true);
		setReason(null);
		setStatus({ kind: "idle" });
	}

	async function createProject(
		parent: string,
		name: string,
		format: Format,
		start: Start | null,
	) {
		setStatus({ kind: "busy" });
		let root: string;
		try {
			root = await invoke<string>("create_project", {
				parent,
				name,
				format,
				start,
			});
		} catch (error) {
			setStatus({ kind: "error", message: failure(error).message });
			return;
		}

		try {
			onOpened(await invoke<OpenProject>("open_project", { root }), true);
		} catch (error) {
			// The project exists now, so the dialog has nothing left to ask:
			// it closes, the shelf shows the new book and the reason is said
			// at the foot.
			setCreating(false);
			invoke<RecentSummary[]>("recent_projects")
				.then(setRecents)
				.catch(() => {});
			setStatus({ kind: "error", message: failure(error).message });
		}
	}

	async function openProject(root: string) {
		setStatus({ kind: "busy" });
		try {
			onOpened(await invoke<OpenProject>("open_project", { root }));
		} catch (error) {
			setStatus({ kind: "error", message: failure(error).message });
		}
	}

	async function browseForProject() {
		const picked = await open({
			directory: true,
			multiple: false,
			title: "Open an Aurora project",
		});
		if (typeof picked === "string") {
			await openProject(picked);
		}
	}

	async function showInFiles(root: string) {
		try {
			await revealItemInDir(root);
		} catch (error) {
			setStatus({ kind: "error", message: failure(error).message });
		}
	}

	// Both ways off the list end the same way, so the list is filtered here
	// rather than read back: the command has already said it worked.
	async function drop(command: string, root: string) {
		setStatus({ kind: "busy" });
		try {
			await invoke(command, { root });
			setRecents((list) =>
				(list ?? []).filter((project) => project.root !== root),
			);
			setAsking(null);
			setStatus({ kind: "idle" });
		} catch (error) {
			setStatus({ kind: "error", message: failure(error).message });
		}
	}

	const all = recents ?? [];
	const counts = formatCounts(all);
	// A format whose last book went falls back to all, so no chip is left
	// pressed over an empty shelf.
	const narrowed = counts[format] > 0 ? format : "all";
	const shown = shelved(all, narrowed, search, sort);
	const chips = (["all", ...Object.keys(FORMATS)] as ShelfFormat[]).filter(
		(each) => each === "all" || counts[each] > 0,
	);

	// Only a chip moves the books; the search changes them at once.
	const shelf = useRef<HTMLUListElement>(null);
	const narrowing = useRef<Spots | null>(null);

	function narrow(to: ShelfFormat) {
		narrowing.current = spots(shelf.current);
		setFormat(to);
	}

	useLayoutEffect(() => {
		if (narrowing.current !== null) {
			settle(shelf.current, narrowing.current, {
				slide: 200,
				enter: 180,
				leave: 100,
				easing: "--ease-out",
			});
			narrowing.current = null;
		}
	}, [narrowed]);

	function showAll() {
		setSearch("");
		setFormat("all");
	}

	return (
		<div className="welcome">
			<div className="welcome__bar">
				<span className="welcome__name">Aurora</span>
				<ThemeToggle
					theme={theme}
					onTheme={onTheme}
					className="welcome__button"
				/>
				<AccountChip
					onLogin={onLogin}
					onProfile={onProfile}
					onSettings={onSettings}
				/>
			</div>

			{drawn && (
				<NewProject
					closing={!creating}
					instant={instant}
					formats={formats}
					structures={structures}
					busy={busy}
					error={status.kind === "error" ? status.message : null}
					onCancel={() => {
						setCreating(false);
						setStatus({ kind: "idle" });
					}}
					onChange={() => setStatus({ kind: "idle" })}
					projectsFolder={projectsFolder}
					home={home}
					onFolder={onProjectsFolder}
					onCreate={(parent, name, format, start) =>
						void createProject(parent, name, format, start)
					}
				/>
			)}

			{/* Only the books scroll: the heading and the tools stay in reach. */}
			<div className="shelf shelf--top">
				<div className="shelf__head">
					<div>
						<h1 className="shelf__heading">Your shelf</h1>
						<p className="shelf__count" aria-live="polite">
							{all.length === 0
								? ""
								: shown.length === all.length
									? books(all.length)
									: `${shown.length.toLocaleString()} of ${books(all.length)}`}
						</p>
					</div>
					<div className="shelf__actions">
						<button
							type="button"
							className="shelf__folder"
							disabled={busy}
							onClick={() => void browseForProject()}
						>
							Open another folder…
						</button>
						<button
							type="button"
							className="welcome__create"
							disabled={busy}
							onClick={startNew}
						>
							New project
						</button>
					</div>
				</div>

				{all.length > 0 && (
					<div className="shelf__tools">
						<div
							className="shelf__chips"
							role="group"
							aria-label="Show"
						>
							{chips.map((each) => (
								<button
									key={each}
									type="button"
									className="shelf__chip"
									aria-pressed={narrowed === each}
									onClick={() => narrow(each)}
								>
									{each !== "all" && (
										<Icon name={FORMATS[each].icon} />
									)}
									{each === "all"
										? "All"
										: FORMATS[each].plural}
									<span className="shelf__chip-count">
										{counts[each].toLocaleString()}
									</span>
								</button>
							))}
						</div>
						<span className="shelf__spacer" />
						<label
							className="shelf__search"
							data-filled={search === "" ? undefined : ""}
						>
							<Icon name="search" />
							<input
								type="text"
								placeholder="Find a book…"
								aria-label="Find a book by title or author"
								autoComplete="off"
								spellCheck={false}
								value={search}
								onChange={(event) =>
									setSearch(event.target.value)
								}
							/>
							<button
								type="button"
								className="shelf__clear"
								aria-label="Clear the search"
								tabIndex={search === "" ? -1 : 0}
								onClick={() => setSearch("")}
							>
								<Icon name="x" />
							</button>
						</label>
						<Menu
							label="Sort the shelf"
							wrapper="shelf__sort-menu"
							className="shelf__sort"
							trigger={
								<>
									Sort: <b>{SORTS[sort]}</b>
									<Icon name="chevron-down" />
								</>
							}
						>
							{(close) =>
								SORT_ORDER.map((each) => (
									<MenuItem
										key={each}
										checked={sort === each}
										onSelect={() => {
											close();
											setSort(each);
										}}
									>
										<span className="menu__tick">
											{sort === each && (
												<Icon name="check" />
											)}
										</span>
										{SORTS[each]}
									</MenuItem>
								))
							}
						</Menu>
						<div
							className="shelf__view"
							role="group"
							aria-label="View"
						>
							<button
								type="button"
								aria-label="Covers"
								aria-pressed={shelfView === "covers"}
								onClick={() => onShelfView("covers")}
							>
								<Icon name="layout-grid" />
							</button>
							<button
								type="button"
								aria-label="List"
								aria-pressed={shelfView === "list"}
								onClick={() => onShelfView("list")}
							>
								<Icon name="list" />
							</button>
						</div>
					</div>
				)}
			</div>

			<div className="shelf__scroll">
				<section className="shelf shelf--books" aria-label="Books">
					{shown.length === 0 && all.length > 0 && (
						<div className="shelf__empty">
							<p>No books match “{search.trim()}”.</p>
							<button
								type="button"
								className="shelf__folder"
								onClick={showAll}
							>
								Show all books
							</button>
						</div>
					)}
					{shown.length > 0 && shelfView === "list" && (
						<div className="shelf__list-head" aria-hidden="true">
							<span />
							<span>Title</span>
							<span className="shelf__cell--format">Format</span>
							<span className="shelf__cell--left">
								Left off at
							</span>
							<span className="shelf__cell--number">Length</span>
							<span className="shelf__cell--number shelf__cell--date">
								Last opened
							</span>
							<span />
						</div>
					)}
					{shown.length > 0 && (
						<ul
							ref={shelf}
							className={
								shelfView === "list"
									? "shelf__list"
									: "shelf__books"
							}
						>
							{shown.map((project) => (
								<Book
									key={project.root}
									project={project}
									view={shelfView}
									busy={busy}
									home={home}
									asking={asking === project.root}
									onOpen={() =>
										void openProject(project.root)
									}
									onShow={() =>
										void showInFiles(project.root)
									}
									onForget={() =>
										void drop(
											"forget_project",
											project.root,
										)
									}
									onAsk={() => setAsking(project.root)}
									onCancel={() => setAsking(null)}
									onDelete={() =>
										void drop("trash_project", project.root)
									}
								/>
							))}
						</ul>
					)}
				</section>
			</div>

			<p
				className={
					status.kind === "error"
						? "welcome__message welcome__message--error"
						: "welcome__message"
				}
				role="alert"
			>
				{status.kind === "error" ? status.message : reason}
			</p>
		</div>
	);
}
