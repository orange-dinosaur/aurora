import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import Welcome from "./Welcome";
import Project from "./Project";
import Settings from "./Settings";
import Login from "./Login";
import Profile from "./Profile";
import type { LastProject, OpenProject, Preferences } from "./types";
import "./App.css";
import { failure } from "./errors";
import { pressed, SETTINGS } from "./formatting";
import { face } from "./lib/typography";

// What Aurora looks like before the store has answered, and what it falls back
// to if the store cannot be read at all.
const DEFAULTS: Preferences = {
	toolbar: true,
	focus: false,
	typewriter: false,
	sidebar: true,
	rightSidebar: false,
	rightSidebarTab: "synopsis",
	sidebarWidth: 248,
	rightSidebarWidth: 300,
	theme: "system",
	manuscriptFont: "newsreader",
	defaultSprint: null,
	defaultSprintUnit: "words",
	defaultTarget: null,
	idleMinutes: 30,
	sessionClock: true,
	measure: 68,
	fontSize: 16,
	lineHeight: 1.7,
};

/** Whether the key that was pressed was pressed into a field. The manuscript
 * is left out on purpose: it is editable too, but it is what the writer is
 * looking at, and settings should open from it like anywhere else. A name
 * being typed is the case that matters, since opening the dialog takes the
 * focus and a half-typed name is abandoned when it goes. */
function inAField(target: EventTarget | null): boolean {
	return (
		target instanceof HTMLElement &&
		(target.tagName === "INPUT" || target.tagName === "TEXTAREA")
	);
}

type Boot =
	| { kind: "loading" }
	| { kind: "welcome"; notice: string | null }
	| { kind: "project"; project: OpenProject };

function App() {
	const [boot, setBoot] = useState<Boot>({ kind: "loading" });
	const [preferences, setPreferences] = useState<Preferences>(DEFAULTS);
	const [settings, setSettings] = useState(false);
	// Over the app rather than instead of it: leaving the project to sign in
	// would close the documents the writer had open.
	const [login, setLogin] = useState(false);
	const [profile, setProfile] = useState(false);

	// Both are read before anything is drawn, so the bar cannot appear and then
	// vanish on a writer who had hidden it. A store that will not answer costs
	// the preferences, not the project.
	useEffect(() => {
		Promise.all([
			invoke<LastProject>("last_project"),
			invoke<Preferences>("read_preferences").catch(() => DEFAULTS),
		])
			.then(([last, saved]) => {
				setPreferences(saved);
				if (last.kind === "open") {
					setBoot({
						kind: "project",
						project: { name: last.name, root: last.root },
					});
				} else if (last.kind === "missing") {
					setBoot({
						kind: "welcome",
						notice: `${last.name} could not be reopened — ${last.root} is no longer there.`,
					});
					void invoke("forget_project", { root: last.root });
				} else {
					setBoot({ kind: "welcome", notice: null });
				}
			})
			.catch((error: unknown) => {
				setBoot({ kind: "welcome", notice: failure(error).message });
			});
	}, []);

	// Bound here rather than in the project, since the welcome screen has the
	// same preferences behind it and nothing else that answers for a key.
	useEffect(() => {
		function onKeyDown(event: KeyboardEvent) {
			if (pressed(event, SETTINGS) && !inAField(event.target)) {
				event.preventDefault();
				setSettings(true);
			}
		}

		window.addEventListener("keydown", onKeyDown);
		return () => window.removeEventListener("keydown", onKeyDown);
	}, []);

	// The stylesheet reads the desktop's setting on its own, so following it
	// means saying nothing rather than saying which.
	useEffect(() => {
		const root = document.documentElement;
		if (preferences.theme === "system") {
			root.removeAttribute("data-theme");
		} else {
			root.setAttribute("data-theme", preferences.theme);
		}
	}, [preferences.theme]);

	// The manuscript is not the only thing set in --serif: a card title and a
	// trashed document's name are too, and they follow it on purpose. It is
	// the one voice the writing has, wherever the writing is named.
	useEffect(() => {
		document.documentElement.style.setProperty(
			"--serif",
			face(preferences.manuscriptFont),
		);
	}, [preferences.manuscriptFont]);

	// A write that fails costs the setting sticking, and neither a writing
	// screen nor the dialog over it has anywhere to say so.
	function save(next: Preferences) {
		setPreferences(next);
		void invoke("write_preferences", { preferences: next });
	}

	return (
		<main className="app">
			{boot.kind === "project" ? (
				<Project
					name={boot.project.name}
					root={boot.project.root}
					preferences={preferences}
					onPreferences={save}
					onSettings={() => setSettings(true)}
					onLogin={() => setLogin(true)}
					onProfile={() => setProfile(true)}
					onClose={() => {
						// Stops the project reopening on launch; it stays in
						// the recent list.
						void invoke("close_project");
						setBoot({ kind: "welcome", notice: null });
					}}
				/>
			) : (
				boot.kind === "welcome" && (
					<Welcome
						notice={boot.notice}
						onOpened={(project) =>
							setBoot({ kind: "project", project })
						}
						onLogin={() => setLogin(true)}
						onProfile={() => setProfile(true)}
						onSettings={() => setSettings(true)}
						theme={preferences.theme}
						onTheme={(theme) => save({ ...preferences, theme })}
					/>
				)
			)}

			{settings && (
				<Settings
					preferences={preferences}
					onPreferences={save}
					onClose={() => setSettings(false)}
					onLogin={() => {
						setSettings(false);
						setLogin(true);
					}}
				/>
			)}

			{login && (
				<Login
					onBack={() => setLogin(false)}
					theme={preferences.theme}
					onTheme={(theme) => save({ ...preferences, theme })}
				/>
			)}
			{profile && (
				<Profile
					onBack={() => setProfile(false)}
					theme={preferences.theme}
					onTheme={(theme) => save({ ...preferences, theme })}
				/>
			)}
		</main>
	);
}

export default App;
