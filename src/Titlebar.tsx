import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { AccountChip } from "./Account";
import Icon from "./Icon";
import Menu, { MenuItem } from "./Menu";
import Session from "./Session";
import { failure } from "./errors";
import { SEARCH, SESSION, shortcutLabel } from "./formatting";
import type { Running } from "./sessions";
import ThemeToggle from "./ThemeToggle";
import type { Branch, Theme } from "./types";

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
	/** Writes every open tab, so asking what is not in a checkpoint sees it. */
	onBeforeSwitch: () => Promise<void>;
	/** Runs a switch the way put back runs, catching the tabs up after it. */
	onSwitch: (task: () => Promise<unknown>) => Promise<void>;
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
	onBeforeSwitch,
	onSwitch,
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
	// none, the switch is already under way.
	async function choose(slug: string): Promise<boolean> {
		await onBeforeSwitch();
		try {
			if (await invoke<boolean>("changes_pending", { root })) {
				return true;
			}
		} catch (error) {
			setMessage(failure(error).message);
			return false;
		}
		void switchTo(slug, null);
		return false;
	}

	async function switchTo(slug: string, checkpoint: string | null) {
		try {
			await onSwitch(async () => {
				if (checkpoint !== null) {
					await invoke("add_checkpoint", {
						root,
						message: checkpoint,
					});
				}
				await invoke("switch_branch", { root, slug });
			});
			setMessage(null);
		} catch (error) {
			setMessage(failure(error).message);
		}
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
							versions={versions}
							current={current}
							close={close}
							onChoose={choose}
							onCheckpoint={(slug, words) =>
								void switchTo(slug, words)
							}
						/>
					)}
				</Menu>
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

// Its own component so the question goes with the menu: the menu unmounts
// what it holds when it closes, however it was closed.
function VersionMenu({
	versions,
	current,
	close,
	onChoose,
	onCheckpoint,
}: {
	versions: Branch[];
	current: Branch;
	close: () => void;
	onChoose: (slug: string) => Promise<boolean>;
	onCheckpoint: (slug: string, message: string) => void;
}) {
	const [asking, setAsking] = useState<string | null>(null);
	const [words, setWords] = useState("");

	if (asking !== null) {
		return (
			<form
				className="titlebar__ask"
				onSubmit={(event) => {
					event.preventDefault();
					if (words.trim() !== "") {
						close();
						onCheckpoint(asking, words);
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
						Add checkpoint and switch
					</button>
				</span>
			</form>
		);
	}

	return versions.map((version) => (
		<MenuItem
			key={version.slug}
			checked={version.current}
			onSelect={() => {
				if (version.current) {
					close();
					return;
				}
				void onChoose(version.slug).then((ask) =>
					ask ? setAsking(version.slug) : close(),
				);
			}}
		>
			<span className="titlebar__tick">
				{version.current && <Icon name="check" />}
			</span>
			{version.name}
		</MenuItem>
	));
}
