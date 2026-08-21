import { useState } from "react";
import Welcome from "./Welcome";
import Project from "./Project";
import "./App.css";

export type OpenProject = {
	name: string;
	root: string;
};

function App() {
	const [project, setProject] = useState<OpenProject | null>(null);

	return (
		<main className="app">
			{project === null ? (
				<Welcome onOpened={setProject} />
			) : (
				<Project
					name={project.name}
					root={project.root}
					onClose={() => setProject(null)}
				/>
			)}
		</main>
	);
}

export default App;
