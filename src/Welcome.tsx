import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import ThemeToggle from "./ThemeToggle";
import type {
	Format,
	FormatLayout,
	OpenProject,
	RecentSummary,
	Theme,
} from "./types";
import { AccountChip } from "./Account";
import { useCoverImage } from "./Cover";
import Icon from "./Icon";
import Menu, { MenuItem, useDismissal } from "./Menu";
import { coverFor } from "./lib/covers";
import { folderName, going } from "./recents";
import { when } from "./dates";
import { failure } from "./errors";

type FormatOption = {
	id: Format;
	name: string;
	description: string;
	folders: string[];
	available: boolean;
};

// Rust owns which formats exist, what they create and whether they are ready.
// The words for each live here, keyed by Rust's list so none can be missing.
const COPY: Record<Format, { name: string; description: string }> = {
	novel: {
		name: "Novel",
		description: "Long-form fiction, organised into chapters.",
	},
	screenplay: {
		name: "Screenplay",
		description: "Film or television, in standard screenplay form.",
	},
	"short-stories": {
		name: "Short Stories",
		description: "A collection of shorter pieces.",
	},
	"stage-play": {
		name: "Stage Play",
		description: "Theatre, organised into acts and scenes.",
	},
};

function describe({ format, folders, available }: FormatLayout): FormatOption {
	return { id: format, ...COPY[format], folders, available };
}

// How big a project is and when it was last open. A project whose manifest
// could not be read shows only the date, rather than a made-up count.
function size(project: RecentSummary) {
	const opened = when(project.lastOpened);
	return project.words === null
		? opened
		: `${project.words.toLocaleString()} words · ${opened}`;
}

type Status =
	{ kind: "idle" } | { kind: "busy" } | { kind: "error"; message: string };

type Props = {
	notice: string | null;
	onOpened: (project: OpenProject) => void;
	onLogin: () => void;
	onProfile: () => void;
	onSettings: () => void;
	theme: Theme;
	onTheme: (theme: Theme) => void;
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
}: {
	project: RecentSummary;
	busy: boolean;
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
}: Props) {
	// Seeded once; the notice describes how this screen was reached.
	const [reason, setReason] = useState(notice);
	const [view, setView] = useState<"shelf" | "new">("shelf");
	const [stage, setStage] = useState<"format" | "details">("format");
	const [selectedId, setSelectedId] = useState<string | null>(null);
	const [formats, setFormats] = useState<FormatOption[]>([]);
	const [name, setName] = useState("");
	const [parent, setParent] = useState<string | null>(null);
	// Null until the list arrives. Counting every project's words takes a
	// moment, and the empty Shelf must not flash up in front of a full one.
	const [recents, setRecents] = useState<RecentSummary[] | null>(null);
	// The project whose book is asking to be deleted. One at a time, since a
	// click anywhere else answers Cancel.
	const [asking, setAsking] = useState<string | null>(null);
	const [status, setStatus] = useState<Status>({ kind: "idle" });

	const chosen = formats.find((format) => format.id === selectedId) ?? null;
	const busy = status.kind === "busy";
	const ready = parent !== null && name.trim() !== "" && !busy;

	useEffect(() => {
		invoke<FormatLayout[]>("format_layouts")
			.then((layouts) => setFormats(layouts.map(describe)))
			.catch((error) =>
				setStatus({ kind: "error", message: failure(error).message }),
			);
	}, []);

	useEffect(() => {
		invoke<RecentSummary[]>("recent_projects")
			.then(setRecents)
			.catch(() => setRecents([]));
	}, []);

	function startNew() {
		setStage("format");
		setView("new");
		setReason(null);
		setStatus({ kind: "idle" });
	}

	async function chooseFolder() {
		const picked = await open({
			directory: true,
			multiple: false,
			title: "Where should the project live?",
		});
		if (typeof picked === "string") {
			setParent(picked);
			setStatus({ kind: "idle" });
		}
	}

	async function createProject() {
		if (chosen === null || parent === null || name.trim() === "") {
			return;
		}

		setStatus({ kind: "busy" });
		try {
			const root = await invoke<string>("create_project", {
				parent,
				name,
				format: chosen.id,
			});
			onOpened(await invoke<OpenProject>("open_project", { root }));
		} catch (error) {
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

			{view === "new" ? (
				<section className="shelf">
					<div className="shelf__head">
						<h1 className="shelf__heading">New project</h1>
					</div>

					{stage === "format" ? (
						<>
							<p className="welcome__subtitle">
								Choose a format and create a new writing
								project.
							</p>

							<fieldset className="formats">
								<legend className="visually-hidden">
									Format
								</legend>
								{formats.map((format) => (
									<label key={format.id} className="format">
										<input
											type="radio"
											name="format"
											className="visually-hidden"
											value={format.id}
											checked={format.id === selectedId}
											disabled={!format.available}
											onChange={() =>
												setSelectedId(format.id)
											}
										/>
										<span className="format__name">
											{format.name}
											{!format.available && (
												<span className="format__soon">
													Soon
												</span>
											)}
										</span>
										<span className="format__description">
											{format.description}
										</span>
										{format.folders.length > 0 && (
											<span className="format__files">
												{format.folders.join(" · ")}
											</span>
										)}
									</label>
								))}
							</fieldset>

							<div className="setup__actions">
								<button
									type="button"
									className="welcome__create"
									disabled={chosen === null || busy}
									onClick={() => setStage("details")}
								>
									Continue
								</button>
								<button
									type="button"
									className="welcome__back"
									disabled={busy}
									onClick={() => {
										setView("shelf");
										setStatus({ kind: "idle" });
									}}
								>
									Back
								</button>
							</div>
						</>
					) : (
						chosen && (
							<>
								<p className="welcome__subtitle">
									Name your {chosen.name.toLowerCase()} and
									choose where it should live.
								</p>

								<form
									className="setup"
									onSubmit={(event) => {
										event.preventDefault();
										void createProject();
									}}
								>
									<label
										className="setup__label"
										htmlFor="project-name"
									>
										Project name
									</label>
									<input
										id="project-name"
										className="setup__input"
										value={name}
										placeholder="Ithaca"
										autoComplete="off"
										autoFocus
										spellCheck={false}
										onChange={(event) => {
											setName(event.target.value);
											setStatus({ kind: "idle" });
										}}
									/>

									<span className="setup__label">
										Location
									</span>
									<button
										type="button"
										className="setup__folder"
										onClick={() => void chooseFolder()}
									>
										{parent ?? "Choose a folder…"}
									</button>

									<p className="setup__note">
										Aurora will create{" "}
										{chosen.folders.join(", ")}.
									</p>

									<div className="setup__actions">
										<button
											type="submit"
											className="welcome__create"
											disabled={!ready}
										>
											{busy
												? "Creating…"
												: "Create project"}
										</button>
										<button
											type="button"
											className="welcome__back"
											disabled={busy}
											onClick={() => {
												setStage("format");
												setStatus({ kind: "idle" });
											}}
										>
											Back
										</button>
									</div>
								</form>
							</>
						)
					)}
				</section>
			) : (
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

					{recents !== null && recents.length === 0 && (
						<div className="shelf__empty">
							<button
								type="button"
								className="shelf__ghost"
								aria-label="New project"
								disabled={busy}
								onClick={startNew}
							>
								<Icon name="plus" />
							</button>
							<p>
								Nothing here yet. Start a book, or open a folder
								you already write in.
							</p>
						</div>
					)}

					{recents !== null && recents.length > 0 && (
						<ul className="shelf__books">
							{recents.map((project) => (
								<Book
									key={project.root}
									project={project}
									busy={busy}
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
			)}

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
