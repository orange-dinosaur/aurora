import { Fragment, useEffect, useRef, useState } from "react";
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
	Structure,
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
import { counts, draftOf, read } from "./lib/start";
import type { Draft } from "./lib/start";

type FormatOption = {
	id: Format;
	name: string;
	/** The format as a sentence would start on it. */
	kind: string;
	description: string;
	folders: string[];
	available: boolean;
};

// Rust owns which formats exist, what they create and whether they are ready.
// The words for each live here, keyed by Rust's list so none can be missing.
const COPY: Record<
	Format,
	{ name: string; kind: string; description: string }
> = {
	novel: {
		name: "Novel",
		kind: "A novel",
		description: "Long-form fiction, organised into chapters.",
	},
	screenplay: {
		name: "Screenplay",
		kind: "A screenplay",
		description: "Film or television, in standard screenplay form.",
	},
	"short-stories": {
		name: "Short Stories",
		kind: "A collection of short stories",
		description: "A collection of shorter pieces.",
	},
	"stage-play": {
		name: "Stage Play",
		kind: "A stage play",
		description: "Theatre, organised into acts and scenes.",
	},
};

function describe({ format, folders, available }: FormatLayout): FormatOption {
	return { id: format, ...COPY[format], folders, available };
}

// As with formats: Rust owns the structures and their numbers, the words live
// here.
const STRUCTURE_COPY: Record<Structure, { name: string; description: string }> =
	{
		"three-acts": {
			name: "Three acts",
			description: "Setup, confrontation, resolution.",
		},
		"four-parts": {
			name: "Four parts",
			description: "Four equal parts, each with a turn.",
		},
		"save-the-cat": {
			name: "Save the Cat",
			description: "Fifteen beats across three acts.",
		},
		"heros-journey": {
			name: "Hero's journey",
			description: "A hero leaves, is tested and returns.",
		},
		"your-own": {
			name: "Your own",
			description: "Numbered parts, as many as you ask for.",
		},
	};

/** A number the writer types on the Start from screen. */
function CountField({
	label,
	value,
	invalid,
	onChange,
}: {
	label: string;
	value: string;
	invalid: boolean;
	onChange: (value: string) => void;
}) {
	return (
		<label className="dialog__label">
			{label}
			<input
				className="dialog__input"
				value={value}
				inputMode="numeric"
				autoComplete="off"
				spellCheck={false}
				aria-invalid={invalid}
				onChange={(event) => onChange(event.target.value)}
			/>
		</label>
	);
}

/** The numbers and ticks under the structure the writer picked. */
function StartFields({
	layout,
	draft,
	onDraft,
	frontMatter,
	backMatter,
	onFrontMatter,
	onBackMatter,
}: {
	layout: StructureLayout;
	draft: Draft;
	onDraft: (draft: Draft) => void;
	frontMatter: boolean;
	backMatter: boolean;
	onFrontMatter: (on: boolean) => void;
	onBackMatter: (on: boolean) => void;
}) {
	const fields = read(draft, layout);
	return (
		<div className="start">
			<div className="start__counts">
				{layout.structure === "your-own" && (
					<CountField
						label="Parts"
						value={draft.parts}
						invalid={fields.parts === null}
						onChange={(parts) => onDraft({ ...draft, parts })}
					/>
				)}
				<CountField
					label="Chapters"
					value={draft.chapters}
					invalid={fields.chapters === null}
					onChange={(chapters) => onDraft({ ...draft, chapters })}
				/>
				<CountField
					label="Scenes per chapter"
					value={draft.scenes}
					invalid={fields.scenes === null}
					onChange={(scenes) => onDraft({ ...draft, scenes })}
				/>
			</div>
			<div className="start__matter">
				<Tick
					label="Front matter"
					on={frontMatter}
					onFlip={() => onFrontMatter(!frontMatter)}
				/>
				<Tick
					label="Back matter"
					on={backMatter}
					onFlip={() => onBackMatter(!backMatter)}
				/>
			</div>
		</div>
	);
}

/** A tick drawn as Export draws its formats, since a native box will not
 * take the app's colours. */
function Tick({
	label,
	on,
	onFlip,
}: {
	label: string;
	on: boolean;
	onFlip: () => void;
}) {
	return (
		<button
			type="button"
			className="export__format"
			role="checkbox"
			aria-checked={on}
			onClick={onFlip}
		>
			<span className="export__tick" />
			{label}
		</button>
	);
}

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

/**
 * The stages of starting a project: format, then for a novel the shape it
 * starts from, then its name. Built when it opens, so a cancelled attempt
 * leaves nothing behind for the next one.
 */
function NewProject({
	formats,
	structures,
	busy,
	onCancel,
	onChange,
	onCreate,
	closing,
	instant,
	asked,
	onFolder,
}: {
	formats: FormatOption[];
	structures: StructureLayout[];
	busy: boolean;
	/** Whether the writer has said where to keep books. Until then the dialog
	 * asks first. */
	asked: boolean;
	onFolder: (folder: string) => void;
	/** Told to close and fading, so it no longer takes anything. */
	closing: boolean;
	/** Opened from the keyboard, so it does not move. */
	instant: boolean;
	onCancel: () => void;
	/** Something was edited, so an earlier failure no longer applies. */
	onChange: () => void;
	onCreate: (
		parent: string,
		name: string,
		format: Format,
		start: Start | null,
	) => void;
}) {
	const [stage, setStage] = useState<
		"folder" | "format" | "start" | "details"
	>(asked ? "format" : "folder");
	// The projects folder on offer. Null until the default arrives, and for
	// good if there is none, which leaves Change… as the way on.
	const [keep, setKeep] = useState<string | null>(null);
	const [selectedId, setSelectedId] = useState(
		formats.find((format) => format.available)?.id ?? null,
	);
	// Null is a single scene, the project as it has always started.
	const [structureId, setStructureId] = useState<Structure | null>(null);
	const [draft, setDraft] = useState<Draft>({
		parts: "",
		chapters: "",
		scenes: "",
	});
	const [frontMatter, setFrontMatter] = useState(false);
	const [backMatter, setBackMatter] = useState(false);
	const [name, setName] = useState("");
	const [parent, setParent] = useState<string | null>(null);
	const dialog = useRef<HTMLDivElement>(null);

	const chosen = formats.find((format) => format.id === selectedId) ?? null;
	// Only a novel has parts and chapters to lay out.
	const shaped = chosen?.id === "novel" && structures.length > 0;
	const layout =
		structures.find((structure) => structure.structure === structureId) ??
		null;
	const numbers = layout === null ? null : counts(draft, layout);
	// A structure whose fields are out of range has nothing to send yet.
	const start: Start | null =
		layout === null || numbers === null
			? null
			: {
					structure: layout.structure,
					...numbers,
					frontMatter,
					backMatter,
				};
	const startReady = layout === null || start !== null;
	const ready = parent !== null && name.trim() !== "" && !busy;

	// Another structure starts from its own numbers. The ticks are not about
	// any one structure, so they stay as they are.
	function chooseStructure(next: StructureLayout | null) {
		setStructureId(next?.structure ?? null);
		if (next !== null) {
			setDraft(draftOf(next));
		}
	}

	// Leaving while a project is being made would hide its failure.
	function leave() {
		if (!busy) {
			onCancel();
		}
	}

	// So a key has somewhere to land: what had focus is behind the dialog now.
	useEffect(() => {
		if (!closing) {
			dialog.current?.focus();
		}
	}, [closing]);

	useEffect(() => {
		function escape(event: KeyboardEvent) {
			if (event.key === "Escape") {
				leave();
			}
		}

		document.addEventListener("keydown", escape);
		return () => document.removeEventListener("keydown", escape);
	});

	useEffect(() => {
		if (!asked) {
			invoke<string>("default_projects_folder").then(setKeep, () => {});
		}
		// Asked once, when the dialog opens.
	}, []);

	async function chooseKeep() {
		const picked = await open({
			directory: true,
			multiple: false,
			title: "Where should Aurora keep your books?",
		});
		if (typeof picked === "string") {
			setKeep(picked);
		}
	}

	async function chooseFolder() {
		const picked = await open({
			directory: true,
			multiple: false,
			title: "Where should the project live?",
		});
		if (typeof picked === "string") {
			setParent(picked);
			onChange();
		}
	}

	return (
		<div
			className="scrim"
			data-closing={closing ? "" : undefined}
			data-instant={instant ? "" : undefined}
			inert={closing}
			// On the press rather than the click, as in Settings, so a selection
			// dragged out of the dialog and released here does not shut it.
			onMouseDown={(event) => {
				if (event.target === event.currentTarget) {
					leave();
				}
			}}
		>
			<div
				className="dialog"
				role="dialog"
				aria-modal="true"
				aria-label="New project"
				ref={dialog}
				tabIndex={-1}
			>
				{stage === "folder" ? (
					<>
						<h2 className="dialog__title">
							Where should Aurora keep your books?
						</h2>
						<p className="dialog__lead">
							Each new project gets a folder of its own in here.
							You can change this later in Settings.
						</p>

						<p className="dialog__saved">
							<span>{keep ?? "No folder chosen"}</span>
							<button
								type="button"
								className="welcome__back"
								onClick={() => void chooseKeep()}
							>
								Change…
							</button>
						</p>

						<div className="dialog__foot">
							<button
								type="button"
								className="welcome__back"
								onClick={onCancel}
							>
								Cancel
							</button>
							<button
								type="button"
								className="welcome__create"
								disabled={keep === null}
								onClick={() => {
									if (keep !== null) {
										onFolder(keep);
										setStage("format");
									}
								}}
							>
								Continue
							</button>
						</div>
					</>
				) : stage === "format" || chosen === null ? (
					<>
						<h2 className="dialog__title">New project</h2>
						<p className="dialog__lead">
							Choose a format and create a new writing project.
						</p>

						<fieldset className="formats">
							<legend className="visually-hidden">Format</legend>
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

						<div className="dialog__foot">
							<button
								type="button"
								className="welcome__back"
								disabled={busy}
								onClick={onCancel}
							>
								Cancel
							</button>
							<button
								type="button"
								className="welcome__create"
								disabled={chosen === null || busy}
								onClick={() =>
									setStage(shaped ? "start" : "details")
								}
							>
								Continue
							</button>
						</div>
					</>
				) : stage === "start" ? (
					<>
						<h2 className="dialog__title">Start from</h2>
						<p className="dialog__lead">
							Choose a shape for the Manuscript.
						</p>

						<fieldset className="formats">
							<legend className="visually-hidden">
								Start from
							</legend>
							<label className="format">
								<input
									type="radio"
									name="structure"
									className="visually-hidden"
									value="single-scene"
									checked={structureId === null}
									onChange={() => chooseStructure(null)}
								/>
								<span className="format__name">
									A single scene
								</span>
								<span className="format__description">
									One empty scene, as today.
								</span>
							</label>
							{structures.map((option) => (
								<Fragment key={option.structure}>
									<label className="format">
										<input
											type="radio"
											name="structure"
											className="visually-hidden"
											value={option.structure}
											checked={
												option.structure === structureId
											}
											onChange={() =>
												chooseStructure(option)
											}
										/>
										<span className="format__name">
											{
												STRUCTURE_COPY[option.structure]
													.name
											}
										</span>
										<span className="format__description">
											{
												STRUCTURE_COPY[option.structure]
													.description
											}
										</span>
										{option.partNames.length > 0 && (
											<span className="format__files">
												{option.partNames.join(" · ")}
											</span>
										)}
									</label>
									{option === layout && (
										<StartFields
											layout={option}
											draft={draft}
											onDraft={setDraft}
											frontMatter={frontMatter}
											backMatter={backMatter}
											onFrontMatter={setFrontMatter}
											onBackMatter={setBackMatter}
										/>
									)}
								</Fragment>
							))}
						</fieldset>

						<div className="dialog__foot">
							<button
								type="button"
								className="welcome__back"
								disabled={busy}
								onClick={() => setStage("format")}
							>
								Back
							</button>
							<button
								type="button"
								className="welcome__create"
								disabled={busy || !startReady}
								onClick={() => setStage("details")}
							>
								Continue
							</button>
						</div>
					</>
				) : (
					<form
						onSubmit={(event) => {
							event.preventDefault();
							if (ready) {
								onCreate(
									parent,
									name,
									chosen.id,
									shaped ? start : null,
								);
							}
						}}
					>
						<h2 className="dialog__title">Name your project</h2>
						<p className="dialog__lead">
							{chosen.kind}, in a folder of its own.
						</p>

						<label className="dialog__label">
							Project name
							<input
								className="dialog__input"
								value={name}
								placeholder="Ithaca"
								autoComplete="off"
								autoFocus
								spellCheck={false}
								onChange={(event) => {
									setName(event.target.value);
									onChange();
								}}
							/>
						</label>

						<div className="dialog__label">
							Location
							<button
								type="button"
								className="dialog__folder"
								onClick={() => void chooseFolder()}
							>
								{parent ?? "Choose a folder…"}
							</button>
						</div>

						<div className="dialog__foot">
							<button
								type="button"
								className="welcome__back"
								disabled={busy}
								onClick={() => {
									setStage(shaped ? "start" : "format");
									onChange();
								}}
							>
								Back
							</button>
							<button
								type="submit"
								className="welcome__create"
								disabled={!ready}
							>
								{busy ? "Creating…" : "Create project"}
							</button>
						</div>
					</form>
				)}
			</div>
		</div>
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

	useEffect(() => {
		invoke<RecentSummary[]>("recent_projects")
			.then(setRecents)
			.catch(() => setRecents([]));
	}, []);

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
		try {
			const root = await invoke<string>("create_project", {
				parent,
				name,
				format,
				start,
			});
			onOpened(await invoke<OpenProject>("open_project", { root }), true);
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

			{drawn && (
				<NewProject
					closing={!creating}
					instant={instant}
					formats={formats}
					structures={structures}
					busy={busy}
					onCancel={() => {
						setCreating(false);
						setStatus({ kind: "idle" });
					}}
					onChange={() => setStatus({ kind: "idle" })}
					asked={projectsFolder !== null}
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
