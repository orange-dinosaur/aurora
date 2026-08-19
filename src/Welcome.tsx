import { useState } from "react";

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

export default function Welcome() {
	const [selectedId, setSelectedId] = useState<string | null>(null);

	return (
		<section className="welcome">
			<h1 className="welcome__title">Aurora</h1>
			<p className="welcome__subtitle">
				Choose a format to start your first writing project.
			</p>

			<ul className="formats">
				{FORMATS.map((format) => (
					<li key={format.id}>
						<button
							className="format"
							disabled={!format.available}
							aria-pressed={format.id === selectedId}
							onClick={() => {
								setSelectedId(format.id);
							}}
						>
							<span className="format__name">
								{format.name}
								{!format.available && (
									<span className="format__soon">Soon</span>
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

			<button className="welcome__create" disabled={selectedId === null}>
				Create project
			</button>
		</section>
	);
}
