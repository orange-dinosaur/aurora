import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { AccountChip } from "./Account";
import Icon from "./Icon";
import Menu, { MenuItem } from "./Menu";
import NameField from "./NameField";
import Session from "./Session";
import { unmergedQuestion } from "./branches";
import { failure } from "./errors";
import { SEARCH, SESSION, shortcutLabel } from "./formatting";
import type { Running } from "./sessions";
import ThemeToggle from "./ThemeToggle";
import type { Branch, Checkpoint, Merging, Theme } from "./types";

type Props = {
	name: string;
	root: string;
	/** The deliberate session running now, or null when there is none. */
	session: Running | null;
	/** Whether the writer wants the session readout drawn here. */
	sessionClock: boolean;
	onStartSession: () => void;
	onStopSession: () => void;
	onTickSession: () => void;
	sidebar: boolean;
	onSidebar: (open: boolean) => void;
	/**
	 * Whether the right sidebar is showing, or null when what is open has
	 * nothing to say about itself and the panel does not apply.
	 */
	rightSidebar: boolean | null;
	onRightSidebar: (open: boolean) => void;
	onSearch: () => void;
	/** The writer's theme preference, which the header can move between two of. */
	theme: Theme;
	onTheme: (theme: Theme) => void;
	onSettings: () => void;
	onLogin: () => void;
	onProfile: () => void;
	/** Leaves the project for the welcome screen. */
	onCloseProject: () => void;
	// The manifest has been read again, so whatever is showing it should look
	// at it afresh.
	onRefreshed: () => void;
	/** Opens compare at a checkpoint, listing the given version's history. */
	onCompare: (checkpoint: string, slug: string) => void;
	/** Writes every open tab, so asking what is not in a checkpoint sees it. */
	onBeforeSwitch: () => Promise<void>;
	/** Runs a switch the way put back runs, catching the tabs up after it. */
	onSwitch: (task: () => Promise<unknown>) => Promise<void>;
	/** The merge waiting to be settled, if there is one. */
	merging: Merging | null;
	/** Reads the merge again after a switch or a merge from here. */
	onMergeChanged: () => Promise<void>;
};

export default function Titlebar({
	name,
	root,
	session,
	sessionClock,
	onStartSession,
	onStopSession,
	onTickSession,
	sidebar,
	onSidebar,
	rightSidebar,
	onRightSidebar,
	onSearch,
	theme,
	onTheme,
	onSettings,
	onLogin,
	onProfile,
	onCloseProject,
	onRefreshed,
	onCompare,
	onBeforeSwitch,
	onSwitch,
	merging,
	onMergeChanged,
}: Props) {
	const [busy, setBusy] = useState(false);
	const [message, setMessage] = useState<string | null>(null);
	const [versions, setVersions] = useState<Branch[]>([]);
	const current = versions.find((version) => version.current);

	async function listVersions() {
		try {
			setVersions(await invoke<Branch[]>("list_branches", { root }));
		} catch (error) {
			setMessage(failure(error).message);
		}
	}

	useEffect(() => {
		void listVersions();
	}, [root]);

	// Whether there are changes the writer has to decide about first. With
	// none, the switch or the merge is already under way.
	async function choose(slug: string, merge: boolean): Promise<boolean> {
		await onBeforeSwitch();
		try {
			if (await invoke<boolean>("changes_pending", { root })) {
				return true;
			}
		} catch (error) {
			setMessage(failure(error).message);
			return false;
		}
		void switchTo(slug, null, merge);
		return false;
	}

	// Merging rewrites files the way switching does, so it catches the tabs
	// up the same way.
	async function switchTo(
		slug: string,
		checkpoint: string | null,
		merge: boolean,
	) {
		try {
			await onSwitch(async () => {
				if (checkpoint !== null) {
					await invoke("add_checkpoint", {
						root,
						message: checkpoint,
					});
				}
				await invoke(merge ? "merge_branch" : "switch_branch", {
					root,
					slug,
				});
			});
			setMessage(null);
		} catch (error) {
			setMessage(failure(error).message);
		}
		await onMergeChanged();
		await listVersions();
	}

	// Hands the project folder to the desktop's file manager. Nothing about
	// the project changes, so a refusal is worth saying once and no more.
	async function reveal() {
		try {
			await revealItemInDir(root);
			setMessage(null);
		} catch (error) {
			setMessage(failure(error).message);
		}
	}

	// `refresh_documents` looks at the folder again before rewriting the
	// manifest, for anything that changed outside Aurora.
	async function refresh() {
		setBusy(true);
		try {
			await invoke("refresh_documents", { root });
			setMessage(null);
			onRefreshed();
		} catch (error) {
			setMessage(failure(error).message);
		} finally {
			setBusy(false);
		}
	}

	return (
		<header className="titlebar">
			{/* The project's name is the way into what can be done to the
			    project as a whole, so it is the trigger rather than a
			    heading. */}
			<Menu
				label={`${name}: project menu`}
				icon="chevron-down"
				text={name}
				trailing
				wrapper="titlebar__menu titlebar__menu--project"
				className="titlebar__name"
			>
				{(close) => (
					<>
						<MenuItem
							onSelect={() => {
								close();
								void reveal();
							}}
						>
							Show in Files
						</MenuItem>
						<span className="menu__rule" />
						<MenuItem
							onSelect={() => {
								close();
								onCloseProject();
							}}
						>
							Close project
						</MenuItem>
					</>
				)}
			</Menu>

			{current !== undefined && (
				<Menu
					label={`Version: ${current.name}`}
					trigger={
						<>
							<span className="titlebar__version-name">
								⎇ {current.name}
							</span>
							<Icon name="chevron-down" />
						</>
					}
					wrapper="titlebar__menu titlebar__menu--version"
					className="titlebar__name titlebar__version"
				>
					{(close) => (
						<VersionMenu
							root={root}
							versions={versions}
							current={current}
							merging={merging !== null}
							close={close}
							onChoose={choose}
							onCheckpoint={(slug, words, merge) =>
								void switchTo(slug, words, merge)
							}
							onCompare={onCompare}
							onChanged={listVersions}
							onFailure={setMessage}
						/>
					)}
				</Menu>
			)}

			{merging !== null && (
				<span className="titlebar__state">
					Merging {merging.name ?? "another version"}
				</span>
			)}

			<span className="titlebar__path" title={root}>
				{root}
			</span>

			<Session
				session={session}
				shown={sessionClock}
				onStart={onStartSession}
				onStop={onStopSession}
				onTick={onTickSession}
				shortcut={shortcutLabel(SESSION)}
			/>

			<span className="titlebar__rule" />

			<div className="titlebar__tools">
				{/* The one place search is reachable from in every
				    configuration: the toolbar and the sidebar can both be
				    hidden, and this cannot. */}
				<button
					type="button"
					className="titlebar__button"
					aria-label="Search this project"
					title={`Search (${shortcutLabel(SEARCH)})`}
					onClick={onSearch}
				>
					<Icon name="search" />
				</button>
				<button
					type="button"
					className="titlebar__button"
					aria-label={busy ? "Refreshing…" : "Refresh documents"}
					disabled={busy}
					onClick={() => void refresh()}
				>
					<Icon name="refresh" />
				</button>

				<ThemeToggle
					theme={theme}
					onTheme={onTheme}
					className="titlebar__button"
				/>

				{/* What looks at the project, then what arranges the window
				    around it. */}
				<span className="titlebar__rule" />

				<button
					type="button"
					className="titlebar__button"
					aria-label={sidebar ? "Hide sidebar" : "Show sidebar"}
					aria-pressed={sidebar}
					onClick={() => onSidebar(!sidebar)}
				>
					<Icon name="panel-left" />
				</button>
				{/* Disabled rather than dropped where it does not apply: a
				    button that comes and goes would shuffle the row every time
				    the writer moved between a document and the trash. */}
				<button
					type="button"
					className="titlebar__button"
					aria-label={
						rightSidebar
							? "Hide right sidebar"
							: "Show right sidebar"
					}
					aria-pressed={rightSidebar === true}
					disabled={rightSidebar === null}
					onClick={() => onRightSidebar(rightSidebar !== true)}
				>
					<Icon name="panel-right" />
				</button>
			</div>

			<AccountChip
				onLogin={onLogin}
				onProfile={onProfile}
				onSettings={onSettings}
			/>

			{/* Out of flow, so a failed refresh cannot push the writing down. */}
			<p className="titlebar__message" role="status">
				{message}
			</p>
		</header>
	);
}

type Doing =
	| { kind: "list" }
	| { kind: "switching"; slug: string; merge: boolean }
	| { kind: "picking" }
	| { kind: "naming" }
	| { kind: "renaming"; slug: string }
	| { kind: "deleting"; version: Branch };

// Its own component so a question or a name field goes with the menu: the menu
// unmounts what it holds when it closes, however it was closed.
function VersionMenu({
	root,
	versions,
	current,
	merging,
	close,
	onChoose,
	onCheckpoint,
	onCompare,
	onChanged,
	onFailure,
}: {
	root: string;
	versions: Branch[];
	current: Branch;
	/** Whether a merge is waiting to be settled, which rules out another. */
	merging: boolean;
	close: () => void;
	/** Switches to the version, or merges it in. True when it has to ask first. */
	onChoose: (slug: string, merge: boolean) => Promise<boolean>;
	onCheckpoint: (slug: string, message: string, merge: boolean) => void;
	onCompare: (checkpoint: string, slug: string) => void;
	/** Lists the versions again after one was made, renamed or deleted. */
	onChanged: () => Promise<void>;
	onFailure: (message: string) => void;
}) {
	const [doing, setDoing] = useState<Doing>({ kind: "list" });
	const [words, setWords] = useState("");
	const [busy, setBusy] = useState(false);
	const [error, setError] = useState<string | null>(null);
	// Whether there is a checkpoint to start a new version from, or null
	// until that is known.
	const [kept, setKept] = useState<boolean | null>(null);

	useEffect(() => {
		let open = true;
		invoke<Checkpoint[]>("list_checkpoints", { root })
			.then((rows) => {
				if (open) {
					setKept(rows.length > 0);
				}
			})
			.catch(() => {
				// Trying will bring back whatever Rust has to say.
				if (open) {
					setKept(true);
				}
			});
		return () => {
			open = false;
		};
	}, [root]);

	function back() {
		setDoing({ kind: "list" });
		setError(null);
	}

	// Runs a name past Rust, keeping the field open with its error on a refusal.
	async function named(task: () => Promise<unknown>, then: () => void) {
		setBusy(true);
		try {
			await task();
			then();
			setError(null);
			await onChanged();
		} catch (failed) {
			setError(failure(failed).message);
		} finally {
			setBusy(false);
		}
	}

	// Opens compare at the version's latest checkpoint, against now.
	async function compare(version: Branch) {
		try {
			const [latest] = await invoke<Checkpoint[]>("list_checkpoints", {
				root,
				version: version.slug,
			});
			if (latest === undefined) {
				onFailure(`${version.name} has no checkpoints yet.`);
				return;
			}
			close();
			onCompare(latest.id, version.slug);
		} catch (failed) {
			onFailure(failure(failed).message);
		}
	}

	async function remove(slug: string) {
		try {
			await invoke("delete_branch", { root, slug });
		} catch (failed) {
			onFailure(failure(failed).message);
		}
		setDoing({ kind: "list" });
		await onChanged();
	}

	if (doing.kind === "deleting") {
		return (
			<div className="titlebar__ask">
				<p className="titlebar__ask-text">
					{unmergedQuestion(
						doing.version.name,
						doing.version.unmerged,
					)}
				</p>
				<span className="titlebar__ask-buttons">
					{/* Focused rather than Delete, so Enter cannot lose work. */}
					<button
						type="button"
						className="sprint__stop"
						autoFocus
						onClick={back}
					>
						Cancel
					</button>
					<button
						type="button"
						className="sprint__stop titlebar__danger"
						onClick={() => void remove(doing.version.slug)}
					>
						Delete
					</button>
				</span>
			</div>
		);
	}

	if (doing.kind === "switching") {
		return (
			<form
				className="titlebar__ask"
				onSubmit={(event) => {
					event.preventDefault();
					if (words.trim() !== "") {
						close();
						onCheckpoint(doing.slug, words, doing.merge);
					}
				}}
			>
				<p className="titlebar__ask-text">
					{current.name} has changes that aren't in a checkpoint.
				</p>
				<input
					className="namefield__input"
					autoFocus
					value={words}
					aria-label="Checkpoint message"
					placeholder="What changed"
					onChange={(event) => setWords(event.target.value)}
				/>
				<span className="titlebar__ask-buttons">
					<button
						type="button"
						className="sprint__stop"
						onClick={close}
					>
						Cancel
					</button>
					<button
						type="submit"
						className="sprint__go"
						disabled={words.trim() === ""}
					>
						{doing.merge
							? "Add checkpoint and merge"
							: "Add checkpoint and switch"}
					</button>
				</span>
			</form>
		);
	}

	function pick(slug: string, merge: boolean) {
		void onChoose(slug, merge).then((ask) =>
			ask ? setDoing({ kind: "switching", slug, merge }) : close(),
		);
	}

	const others = versions.filter((version) => !version.current);

	if (doing.kind === "picking") {
		return (
			<>
				<p className="titlebar__note">
					Which version goes into {current.name}?
				</p>
				{others.map((version) => (
					<MenuItem
						key={version.slug}
						onSelect={() => pick(version.slug, true)}
					>
						<span className="menu__tick" />
						{version.name}
					</MenuItem>
				))}
			</>
		);
	}

	return (
		<>
			{versions.map((version) =>
				doing.kind === "renaming" && doing.slug === version.slug ? (
					<div key={version.slug} className="titlebar__field">
						<NameField
							label={`New name for ${version.name}`}
							placeholder="Name this version"
							initial={version.name}
							busy={busy}
							working="Renaming…"
							error={error}
							onSubmit={(name) =>
								void named(
									() =>
										invoke("rename_branch", {
											root,
											slug: version.slug,
											name,
										}),
									back,
								)
							}
							onCancel={back}
						/>
					</div>
				) : (
					<div key={version.slug} className="titlebar__row">
						<MenuItem
							checked={version.current}
							onSelect={() => {
								if (version.current) {
									close();
									return;
								}
								pick(version.slug, false);
							}}
						>
							<span className="menu__tick">
								{version.current && <Icon name="check" />}
							</span>
							{version.name}
						</MenuItem>
						{(!version.fixed || !version.current) && (
							<span className="titlebar__row-actions">
								{!version.current && (
									<button
										type="button"
										className="menu__trigger"
										aria-label={`Compare with ${version.name}`}
										title="Compare with…"
										onClick={() => void compare(version)}
									>
										<Icon name="arrow-left-right" />
									</button>
								)}
								{!version.fixed && (
									<button
										type="button"
										className="menu__trigger"
										aria-label={`Rename ${version.name}`}
										title="Rename"
										onClick={() =>
											setDoing({
												kind: "renaming",
												slug: version.slug,
											})
										}
									>
										<Icon name="pencil" />
									</button>
								)}
								{!version.current && !version.fixed && (
									<button
										type="button"
										className="menu__trigger"
										aria-label={`Delete ${version.name}`}
										title="Delete"
										onClick={() =>
											version.unmerged > 0
												? setDoing({
														kind: "deleting",
														version,
													})
												: void remove(version.slug)
										}
									>
										<Icon name="trash" />
									</button>
								)}
							</span>
						)}
					</div>
				),
			)}

			<span className="menu__rule" />

			{doing.kind === "naming" ? (
				<div className="titlebar__field">
					<NameField
						label="Name of the new version"
						placeholder="Name the new version"
						busy={busy}
						working="Starting…"
						error={error}
						onSubmit={(name) =>
							void named(
								() => invoke("create_branch", { root, name }),
								close,
							)
						}
						onCancel={back}
					/>
				</div>
			) : (
				<>
					<MenuItem
						disabled={kept !== true}
						onSelect={() => setDoing({ kind: "naming" })}
					>
						New version…
					</MenuItem>
					{kept === false && (
						<p className="titlebar__note">
							Add a first checkpoint to start another version.
						</p>
					)}
					<MenuItem
						disabled={others.length === 0 || merging}
						onSelect={() => setDoing({ kind: "picking" })}
					>
						Merge into {current.name}…
					</MenuItem>
					{merging && (
						<p className="titlebar__note">
							Settle or abort the merge under way first.
						</p>
					)}
				</>
			)}
		</>
	);
}
