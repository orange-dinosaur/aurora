import { useEffect, useRef, useState } from "react";
import type { MouseEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { homeDir } from "@tauri-apps/api/path";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import ThemeToggle from "./ThemeToggle";
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
import Icon from "./Icon";
import Menu, {
	DIALOG_FADE,
	MenuItem,
	useDismissal,
	useLingering,
} from "./Menu";
import { coverFor } from "./lib/covers";
import { folderName, going, place } from "./recents";
import { when } from "./dates";
import { failure } from "./errors";
import NewProject, { COPY, describe, type FormatOption } from "./NewProject";

// How big a project is and when it was last open. A project whose manifest
// could not be read shows only the date, rather than a made-up count.
function size(project: RecentSummary) {
	const words =
		project.words === null
			? null
			: `${project.words.toLocaleString()} words`;
	const opened =
		project.lastOpened === null ? null : when(project.lastOpened);
	return [words, opened].filter(Boolean).join(" · ");
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
	/** Where new projects go, or null while the writer has never been asked. */
	projectsFolder: string | null;
	onProjectsFolder: (folder: string) => void;
};

/** One project standing on the Shelf. */
function Book({
	project,
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
	// A book nobody has named yet goes by its project's name. One that cannot
	// be read has only its folder to go by: the name may be as stale as the
	// file that could not be read.
	const title = unread
		? folderName(project.root)
		: project.title?.trim() || project.name;

	return (
		<li className="shelf__book">
			{/* Still a button when it cannot be read: opening it is how the
			    writer finds out why, at the foot of the window. */}
			<button
				type="button"
				className="shelf__open"
				disabled={busy}
				onClick={onOpen}
			>
				{unread ? (
					<span className="shelf__cover shelf__cover--unread">
						<strong>{title}</strong>
						<span>Aurora can't read this project.</span>
					</span>
				) : (
					/* A wash of the title stands behind an image too, so the
					   cover is never blank while one loads or if it cannot be
					   read. */
					<span
						className={
							src === ""
								? "shelf__cover"
								: "shelf__cover shelf__cover--image"
						}
						style={{ background: coverFor(title) }}
					>
						{src !== "" && <img src={src} alt="" />}
						<span className="shelf__title">{title}</span>
						<span className="shelf__author">{project.author}</span>
					</span>
				)}
				{project.format !== null && (
					<span className="shelf__format">
						{COPY[project.format].name}
					</span>
				)}
				<span className="shelf__meta">{size(project)}</span>
				{project.leftOff !== null && (
					<span
						className="shelf__meta shelf__left"
						title={project.leftOff}
					>
						{project.leftOff}
					</span>
				)}
				{project.elsewhere && (
					<span className="shelf__meta shelf__place">
						<Icon name="link" />
						<span>{place(project.root, home)}</span>
					</span>
				)}
			</button>

			{asking && (
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
			)}

			<Menu label={`Actions for ${title}`} icon="more-horizontal">
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

			<section className="shelf">
				<div className="shelf__head">
					<div>
						<h1 className="shelf__heading">Your shelf</h1>
						{recents !== null && recents.length > 0 && (
							<p className="shelf__count">
								{recents.length === 1
									? "1 book"
									: `${recents.length.toLocaleString()} books`}
							</p>
						)}
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

				{recents !== null && (
					<ul className="shelf__books">
						<li>
							<button
								type="button"
								className="shelf__ghost"
								aria-label="New project"
								disabled={busy}
								onClick={startNew}
							>
								<Icon name="plus" />
								<span className="shelf__ghost-label">
									New project
								</span>
							</button>
						</li>
						{recents.map((project) => (
							<Book
								key={project.root}
								project={project}
								busy={busy}
								home={home}
								asking={asking === project.root}
								onOpen={() => void openProject(project.root)}
								onShow={() => void showInFiles(project.root)}
								onForget={() =>
									void drop("forget_project", project.root)
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
