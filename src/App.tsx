import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import Welcome from "./Welcome";
import Project from "./Project";
import type { LastProject, OpenProject, Preferences } from "./types";
import "./App.css";
import { failure } from "./errors";

// What Aurora looks like before the store has answered, and what it falls back
// to if the store cannot be read at all.
const DEFAULTS: Preferences = {
	toolbar: true,
	focus: false,
	typewriter: false,
	outline: false,
	sidebar: true,
	rightSidebar: false,
	rightSidebarTab: "synopsis",
	measure: 68,
	fontSize: 16,
	lineHeight: 1.7,
};

type Boot =
	| { kind: "loading" }
	| { kind: "welcome"; notice: string | null }
	| { kind: "project"; project: OpenProject };

function App() {
	const [boot, setBoot] = useState<Boot>({ kind: "loading" });
	const [preferences, setPreferences] = useState<Preferences>(DEFAULTS);

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

	return (
		<main className="app">
			{boot.kind === "project" ? (
				<Project
					name={boot.project.name}
					root={boot.project.root}
					preferences={preferences}
					onPreferences={(next) => {
						setPreferences(next);
						// A write that fails costs the setting sticking, and
						// there is nowhere in a writing screen to say so.
						void invoke("write_preferences", {
							preferences: next,
						});
					}}
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
					/>
				)
			)}
		</main>
	);
}

export default App;
