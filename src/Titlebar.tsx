import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AccountChip } from "./Account";
import Icon from "./Icon";
import Menu, { MenuItem } from "./Menu";
import Session from "./Session";
import { failure } from "./errors";
import { SEARCH, SESSION, shortcutLabel } from "./formatting";
import type { Running } from "./sessions";

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
	onSettings: () => void;
	onLogin: () => void;
	/** Leaves the project for the welcome screen. */
	onCloseProject: () => void;
	// The manifest has been read again, so whatever is showing it should look
	// at it afresh.
	onRefreshed: () => void;
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
	onSettings,
	onLogin,
	onCloseProject,
	onRefreshed,
}: Props) {
	const [busy, setBusy] = useState(false);
	const [message, setMessage] = useState<string | null>(null);

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
								onSettings();
							}}
						>
							Settings
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

			<AccountChip onLogin={onLogin} />

			{/* Out of flow, so a failed refresh cannot push the writing down. */}
			<p className="titlebar__message" role="status">
				{message}
			</p>
		</header>
	);
}
