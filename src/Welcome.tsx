import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { OpenProject } from "./App";

type Format = {
	id: string;
	name: string;
	description: string;
	files: string[];
	available: boolean;
};

const FORMATS: Format[] = [
	{
		id: "novel",
		name: "Novel",
		description: "Long-form fiction, organised into chapters.",
		files: ["Manuscript", "Outline", "Characters", "Locations", "Notes"],
		available: true,
	},
	{
		id: "screenplay",
		name: "Screenplay",
		description: "Film or television, in standard screenplay form.",
		files: ["Script", "Beat Sheet", "Characters", "Notes"],
		available: false,
	},
	{
		id: "short-stories",
		name: "Short Stories",
		description: "A collection of shorter pieces.",
		files: ["Stories", "Ideas", "Notes"],
		available: false,
	},
	{
		id: "stage-play",
		name: "Stage Play",
		description: "Theatre, organised into acts and scenes.",
		files: ["Script", "Characters", "Staging", "Notes"],
		available: false,
	},
];

type RecentProject = {
	name: string;
	root: string;
	lastOpened: string;
};

type Status =
	{ kind: "idle" } | { kind: "busy" } | { kind: "error"; message: string };

type Props = {
	notice: string | null;
	onOpened: (project: OpenProject) => void;
};

export default function Welcome({ notice, onOpened }: Props) {
	// Seeded once; the notice describes how this screen was reached.
	const [reason, setReason] = useState(notice);
	const [stage, setStage] = useState<"format" | "details">("format");
	const [selectedId, setSelectedId] = useState<string | null>(null);
	const [name, setName] = useState("");
	const [parent, setParent] = useState<string | null>(null);
	const [recents, setRecents] = useState<RecentProject[]>([]);
	const [status, setStatus] = useState<Status>({ kind: "idle" });

	const chosen = FORMATS.find((format) => format.id === selectedId) ?? null;
	const busy = status.kind === "busy";
	const ready = parent !== null && name.trim() !== "" && !busy;

	useEffect(() => {
		invoke<RecentProject[]>("recent_projects")
			.then(setRecents)
			.catch(() => setRecents([]));
	}, []);

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
			onOpened({ name, root });
		} catch (error) {
			setStatus({ kind: "error", message: String(error) });
		}
	}

	async function openProject(root: string) {
		setStatus({ kind: "busy" });
		try {
			onOpened(await invoke<OpenProject>("open_project", { root }));
		} catch (error) {
			setStatus({ kind: "error", message: String(error) });
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

	return (
		<section className="welcome">
			<h1 className="welcome__title">Aurora</h1>

			{stage === "format" ? (
				<>
					<p className="welcome__subtitle">
						Choose a format and create a new writing project.
					</p>

					<ul className="formats">
						{FORMATS.map((format) => (
							<li key={format.id}>
								<button
									type="button"
									className="format"
									disabled={!format.available}
									aria-pressed={format.id === selectedId}
									onClick={() => setSelectedId(format.id)}
								>
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
									<span className="format__files">
										{format.files.join(" · ")}
									</span>
								</button>
							</li>
						))}
					</ul>

					<button
						type="button"
						className="welcome__create"
						disabled={chosen === null || busy}
						onClick={() => {
							setStage("details");
							setReason(null);
						}}
					>
						Continue
					</button>

					<div className="existing">
						<h2 className="existing__title">Or open an existing project</h2>

						{recents.length > 0 && (
							<ul className="recents">
								{recents.map((project) => (
									<li key={project.root}>
										<button
											type="button"
											className="recent"
											disabled={busy}
											onClick={() =>
												void openProject(project.root)
											}
										>
											<span className="recent__name">
												{project.name}
											</span>
											<span className="recent__path">
												{project.root}
											</span>
										</button>
									</li>
								))}
							</ul>
						)}

						<button
							type="button"
							className="welcome__back"
							disabled={busy}
							onClick={() => void browseForProject()}
						>
							Select folder…
						</button>
					</div>
				</>
			) : (
				chosen && (
					<>
						<p className="welcome__subtitle">
							Name your {chosen.name.toLowerCase()} and choose
							where it should live.
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

							<span className="setup__label">Location</span>
							<button
								type="button"
								className="setup__folder"
								onClick={() => void chooseFolder()}
							>
								{parent ?? "Choose a folder…"}
							</button>

							<p className="setup__note">
								Aurora will create {chosen.files.join(", ")}.
							</p>

							<div className="setup__actions">
								<button
									type="submit"
									className="welcome__create"
									disabled={!ready}
								>
									{busy ? "Creating…" : "Create project"}
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
		</section>
	);
}
