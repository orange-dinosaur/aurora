import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import type { FormatLayout, OpenProject, RecentSummary } from "./types";
import { AccountChip } from "./Account";
import Menu, { MenuItem } from "./Menu";
import { contents } from "./recents";
import { when } from "./dates";
import { failure } from "./errors";

type Format = {
	id: string;
	name: string;
	description: string;
	folders: string[];
	available: boolean;
};

// Rust owns which formats exist, what they create and whether they can be
const COPY: Record<string, { name: string; description: string } | undefined> =
	{
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

function describe({ format, folders, available }: FormatLayout): Format {
	const copy = COPY[format];
	return {
		id: format,
		name: copy?.name ?? format,
		description: copy?.description ?? "",
		folders,
		available,
	};
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
};

export default function Welcome({
	notice,
	onOpened,
	onLogin,
	onProfile,
	onSettings,
}: Props) {
	// Seeded once; the notice describes how this screen was reached.
	const [reason, setReason] = useState(notice);
	const [stage, setStage] = useState<"format" | "details">("format");
	const [selectedId, setSelectedId] = useState<string | null>(null);
	const [formats, setFormats] = useState<Format[]>([]);
	const [name, setName] = useState("");
	const [parent, setParent] = useState<string | null>(null);
	const [recents, setRecents] = useState<RecentSummary[]>([]);
	// The project whose row has turned into a question. One at a time: the
	// question stands where the row was, so two of them could not both.
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
				list.filter((project) => project.root !== root),
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
				<AccountChip
					onLogin={onLogin}
					onProfile={onProfile}
					onSettings={onSettings}
				/>
			</div>

			<section className="welcome__columns">
				<div className="welcome__main">
					<h1 className="welcome__title">Aurora</h1>

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
				</div>

				{/* Outside the stage, so choosing a format never takes the
				    writer's own projects off the screen. */}
				<aside className="opening">
					<h2 className="opening__title">Your projects</h2>

					{recents.length === 0 ? (
						<p className="opening__none">
							Nothing has been opened yet.
						</p>
					) : (
						<ul className="recents">
							{recents.map((project) =>
								asking === project.root ? (
									// The question stands where the row was, so
									// nothing else on the screen moves while it
									// is being answered.
									<li
										key={project.root}
										className="recents__ask"
									>
										<p className="recents__question">
											Move <strong>{project.name}</strong>{" "}
											{contents(project.documents)} to the
											system trash?
										</p>
										<div className="recents__answers">
											<button
												type="button"
												className="recents__keep"
												disabled={busy}
												onClick={() => setAsking(null)}
											>
												Cancel
											</button>
											<button
												type="button"
												className="recents__bin"
												disabled={busy}
												onClick={() =>
													void drop(
														"trash_project",
														project.root,
													)
												}
											>
												Delete project
											</button>
										</div>
									</li>
								) : (
									<li
										key={project.root}
										className="recents__item"
									>
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
											<span className="recent__size">
												{size(project)}
											</span>
											<span className="recent__path">
												{project.root}
											</span>
										</button>

										<Menu
											label={`Actions for ${project.name}`}
											icon="more-vertical"
										>
											{(close) => (
												<>
													<MenuItem
														onSelect={() => {
															close();
															void showInFiles(
																project.root,
															);
														}}
													>
														Show in Files
													</MenuItem>
													<MenuItem
														onSelect={() => {
															close();
															void drop(
																"forget_project",
																project.root,
															);
														}}
													>
														Remove from Aurora
														<span className="menu__hint">
															Leaves the files
															where they are
														</span>
													</MenuItem>

													<span className="menu__rule" />

													<MenuItem
														danger
														onSelect={() => {
															close();
															setAsking(
																project.root,
															);
														}}
													>
														Delete project…
														<span className="menu__hint">
															Moves the whole
															folder to the trash
														</span>
													</MenuItem>
												</>
											)}
										</Menu>
									</li>
								),
							)}
						</ul>
					)}

					<button
						type="button"
						className="welcome__back"
						disabled={busy}
						onClick={() => void browseForProject()}
					>
						Open another folder…
					</button>
				</aside>
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
