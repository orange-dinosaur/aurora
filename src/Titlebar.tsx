import { useEffect, useState } from "react";
import type { ReactNode, RefObject } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AccountChip } from "./Account";
import Icon from "./Icon";
import Session from "./Session";
import { SEARCH, SESSION, shortcutLabel } from "./formatting";
import { dayTally } from "./lib/stats";
import type { Limit, Running, Sessions } from "./sessions";
import ThemeToggle from "./ThemeToggle";
import type { Merging, PastSession, SprintUnit, Theme } from "./types";

type Props = {
	root: string;
	/** The deliberate session running now, or null when there is none. */
	session: Running | null;
	/** Whether the writer wants the session readout drawn here. */
	sessionClock: boolean;
	/** What the sprint field starts on, from the writer's preferences. */
	defaultSprint: number | null;
	defaultSprintUnit: SprintUnit;
	/** Opens a deliberate session. A limit makes it a sprint. */
	onStartSession: (limit?: Limit) => void;
	onStopSession: () => void;
	onTickSession: () => void;
	/** The two layers as they stand, for today's words not on disk yet. */
	sessions: Sessions;
	/** What the whole project holds. */
	words: number;
	/** Bumped when a session has been written to the history file. */
	logged: number;
	sidebar: boolean;
	onSidebar: (open: boolean) => void;
	/** The sidebar's button, which a floating sidebar treats as part of itself. */
	sidebarButton: RefObject<HTMLButtonElement | null>;
	/** Off during Compare and Merge, whose lists stand where the sidebar stands. */
	sidebarToggle: boolean;
	/** The open tabs, or the way back from Compare or Merge. */
	tabs: ReactNode;
	onSearch: () => void;
	/** The writer's theme preference, which the header can move between two of. */
	theme: Theme;
	onTheme: (theme: Theme) => void;
	onSettings: (keyboard: boolean) => void;
	onLogin: () => void;
	onProfile: () => void;
	/** The merge waiting to be settled, if there is one. */
	merging: Merging | null;
};

export default function Titlebar({
	root,
	session,
	sessionClock,
	defaultSprint,
	defaultSprintUnit,
	onStartSession,
	onStopSession,
	onTickSession,
	sessions,
	words,
	logged,
	sidebar,
	onSidebar,
	sidebarButton,
	sidebarToggle,
	tabs,
	onSearch,
	theme,
	onTheme,
	onSettings,
	onLogin,
	onProfile,
	merging,
}: Props) {
	return (
		<header className="titlebar">
			{sidebarToggle && (
				<button
					ref={sidebarButton}
					type="button"
					className="titlebar__button"
					aria-label={sidebar ? "Hide sidebar" : "Show sidebar"}
					aria-pressed={sidebar}
					onClick={() => onSidebar(!sidebar)}
				>
					<Icon name="panel-left" />
				</button>
			)}

			{merging !== null && (
				<span className="titlebar__state">
					Merging {merging.name ?? "another version"}
				</span>
			)}

			{/* Drawn with nothing in it too, so what follows keeps its place. */}
			<div className="titlebar__tabs">{tabs}</div>

			{/* Never shrinks, so these hold the window's right edge however
			    little room the row has. */}
			<div className="titlebar__right">
				<Today
					root={root}
					sessions={sessions}
					words={words}
					logged={logged}
				/>

				<Session
					session={session}
					shown={sessionClock}
					defaultAmount={defaultSprint}
					defaultUnit={defaultSprintUnit}
					onStart={onStartSession}
					onStop={onStopSession}
					onTick={onTickSession}
					shortcut={shortcutLabel(SESSION)}
				/>

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

				<ThemeToggle
					theme={theme}
					onTheme={onTheme}
					className="titlebar__button"
				/>

				<AccountChip
					onLogin={onLogin}
					onProfile={onProfile}
					onSettings={onSettings}
				/>
			</div>
		</header>
	);
}

/**
 * The words written across the project today. The automatic layer runs whether
 * or not the writer opened a session, so it is the one number that holds them
 * all. A history that cannot be read leaves this empty: the Stats tab says why.
 */
function Today({
	root,
	sessions,
	words,
	logged,
}: {
	root: string;
	sessions: Sessions;
	words: number;
	logged: number;
}) {
	const [history, setHistory] = useState<PastSession[] | null>(null);

	useEffect(() => {
		let listening = true;
		invoke<PastSession[]>("read_history", { root })
			.then((read) => {
				if (listening) {
					setHistory(read);
				}
			})
			.catch(() => {
				if (listening) {
					setHistory(null);
				}
			});

		return () => {
			listening = false;
		};
	}, [root, logged]);

	if (history === null) {
		return null;
	}

	const at = Date.now();
	const { written } = dayTally(history, sessions, words, at).automatic;

	return (
		<span
			className="titlebar__today"
			title="Words written in this project today"
		>
			{written.toLocaleString()} today
		</span>
	);
}
