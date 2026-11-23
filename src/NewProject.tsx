import { Fragment, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type {
	Format,
	FormatLayout,
	Start,
	Structure,
	StructureLayout,
} from "./types";
import Dialog from "./Dialog";
import { tilde } from "./recents";
import { counts, draftOf, read } from "./lib/start";
import type { Draft } from "./lib/start";

export type FormatOption = {
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
export const COPY: Record<
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
		description: "Film or television, organised into acts and scenes.",
	},
	"short-stories": {
		name: "Short Stories",
		kind: "A collection of short stories",
		description: "Separate stories in one book, each with its own scenes.",
	},
	"stage-play": {
		name: "Stage Play",
		kind: "A stage play",
		description: "Theatre, organised into acts and scenes.",
	},
};

export function describe({
	format,
	folders,
	available,
}: FormatLayout): FormatOption {
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

/** The numbers and ticks under the structure the writer picked. A screenplay
 * has no chapters and no matter, so it asks only for scenes. */
function StartFields({
	layout,
	screenplay,
	draft,
	onDraft,
	frontMatter,
	backMatter,
	onFrontMatter,
	onBackMatter,
}: {
	layout: StructureLayout;
	screenplay: boolean;
	draft: Draft;
	onDraft: (draft: Draft) => void;
	frontMatter: boolean;
	backMatter: boolean;
	onFrontMatter: (on: boolean) => void;
	onBackMatter: (on: boolean) => void;
}) {
	const fields = read(draft, layout, screenplay);
	const start = useRef<HTMLDivElement>(null);

	// Opening a structure low in the list would leave its fields under the
	// dialog's foot. A frame later, so the scroll that brings the clicked
	// radio into view cannot undo this one.
	useEffect(() => {
		const frame = requestAnimationFrame(() =>
			start.current?.scrollIntoView({ block: "nearest" }),
		);
		return () => cancelAnimationFrame(frame);
	}, []);

	return (
		<div className="start" ref={start}>
			<div className="start__counts">
				{layout.structure === "your-own" && (
					<CountField
						label="Parts"
						value={draft.parts}
						invalid={fields.parts === null}
						onChange={(parts) => onDraft({ ...draft, parts })}
					/>
				)}
				{!screenplay && (
					<CountField
						label="Chapters"
						value={draft.chapters}
						invalid={fields.chapters === null}
						onChange={(chapters) => onDraft({ ...draft, chapters })}
					/>
				)}
				<CountField
					label={screenplay ? "Scenes per act" : "Scenes per chapter"}
					value={draft.scenes}
					invalid={fields.scenes === null}
					onChange={(scenes) => onDraft({ ...draft, scenes })}
				/>
			</div>
			{!screenplay && (
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
			)}
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

/**
 * The stages of starting a project: format, then for a novel or a screenplay
 * the shape it starts from, then its name. Built when it opens, so a cancelled attempt
 * leaves nothing behind for the next one.
 */
export default function NewProject({
	formats,
	structures,
	busy,
	error,
	onCancel,
	onChange,
	onCreate,
	closing,
	instant,
	projectsFolder,
	onFolder,
	home,
}: {
	/** For writing a folder under home with `~`. */
	home: string | null;
	formats: FormatOption[];
	structures: StructureLayout[];
	busy: boolean;
	/** Why the last attempt was refused, or null. */
	error: string | null;
	/** Where new projects go, or null until the writer has said, in which
	 * case the dialog asks first. */
	projectsFolder: string | null;
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
	>(projectsFolder === null ? "folder" : "format");
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
	// Where this one project goes: the projects folder unless Change… says
	// otherwise, which leaves the setting alone.
	const [parent, setParent] = useState<string | null>(projectsFolder);

	const chosen = formats.find((format) => format.id === selectedId) ?? null;
	// A collection starts as one story and has no shape to choose.
	const screenplay = chosen?.id === "screenplay";
	const shaped =
		(chosen?.id === "novel" || screenplay) && structures.length > 0;
	const layout =
		structures.find((structure) => structure.structure === structureId) ??
		null;
	const numbers = layout === null ? null : counts(draft, layout, screenplay);
	// A structure whose fields are out of range has nothing to send yet.
	const start: Start | null =
		layout === null || numbers === null
			? null
			: {
					structure: layout.structure,
					...numbers,
					frontMatter: frontMatter && !screenplay,
					backMatter: backMatter && !screenplay,
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

	useEffect(() => {
		if (projectsFolder === null) {
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
		<Dialog
			label="New project"
			className="dialog"
			closing={closing}
			instant={instant}
			onLeave={leave}
		>
			{stage === "folder" ? (
				<>
					<h2 className="dialog__title">
						Where should Aurora keep your books?
					</h2>
					<p className="dialog__lead">
						Each new project gets a folder of its own in here. You
						can change this later in Settings.
					</p>

					<p className="dialog__saved">
						<span>
							{keep === null
								? "No folder chosen"
								: tilde(keep, home)}
						</span>
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
									setParent(keep);
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
									onChange={() => setSelectedId(format.id)}
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
						<legend className="visually-hidden">Start from</legend>
						<label className="format">
							<input
								type="radio"
								name="structure"
								className="visually-hidden"
								value="single-scene"
								checked={structureId === null}
								onChange={() => chooseStructure(null)}
							/>
							<span className="format__name">A single scene</span>
							<span className="format__description">
								{screenplay
									? "One act holding one scene."
									: "One empty scene, as today."}
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
										onChange={() => chooseStructure(option)}
									/>
									<span className="format__name">
										{STRUCTURE_COPY[option.structure].name}
									</span>
									<span className="format__description">
										{
											STRUCTURE_COPY[option.structure]
												.description
										}
									</span>
									{option.partNames.length > 0 && (
										<span className="format__files">
											{screenplay
												? `${option.partNames.length} acts`
												: option.partNames.join(" · ")}
										</span>
									)}
								</label>
								{option === layout && (
									<StartFields
										layout={option}
										screenplay={screenplay}
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

					<p className="dialog__saved">
						{parent !== null && (
							<span>Saved in {tilde(parent, home)}</span>
						)}
						<button
							type="button"
							className="welcome__back"
							onClick={() => void chooseFolder()}
						>
							Change…
						</button>
					</p>

					<p className="dialog__problem" role="alert">
						{error}
					</p>

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
		</Dialog>
	);
}
