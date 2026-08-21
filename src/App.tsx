import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import Welcome from "./Welcome";
import Project from "./Project";
import "./App.css";

export type OpenProject = {
	name: string;
	root: string;
};

type LastProject =
	| { kind: "none" }
	| { kind: "open"; name: string; root: string }
	| { kind: "missing"; name: string; root: string };

type Boot =
	| { kind: "loading" }
	| { kind: "welcome"; notice: string | null }
	| { kind: "project"; project: OpenProject };

function App() {
	const [boot, setBoot] = useState<Boot>({ kind: "loading" });

	useEffect(() => {
		invoke<LastProject>("last_project")
			.then((last) => {
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
				setBoot({ kind: "welcome", notice: String(error) });
			});
	}, []);

	return (
		<main className="app">
			{boot.kind === "project" ? (
				<Project
					name={boot.project.name}
					root={boot.project.root}
					onClose={() => setBoot({ kind: "welcome", notice: null })}
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
