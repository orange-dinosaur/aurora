// The question before a put back that would write over changes not in a
// checkpoint. It floats over the window, so asking never moves what is under it.

import { useRef, useState } from "react";
import { useDismissal } from "./Menu";
import { overwriteQuestion } from "./lib/versions";

export default function Overwrite({
	names,
	onAnswer,
}: {
	names: string[];
	onAnswer: (go: boolean) => void;
}) {
	const [listed, setListed] = useState(false);
	const box = useRef<HTMLDivElement>(null);

	// A click anywhere else cancels, as it does for a menu.
	useDismissal(true, box, () => onAnswer(false));

	return (
		<div
			ref={box}
			className="overwrite"
			role="alertdialog"
			aria-label="Put back"
		>
			<p className="overwrite__text">{overwriteQuestion(names)}</p>
			{names.length > 1 && (
				<button
					type="button"
					className="versions__action"
					aria-expanded={listed}
					onClick={() => setListed(!listed)}
				>
					{listed ? "Hide the list" : "Show the list"}
				</button>
			)}
			{listed && (
				<ul className="overwrite__list">
					{names.map((name, index) => (
						<li key={index}>{name}</li>
					))}
				</ul>
			)}
			<span className="overwrite__buttons">
				{/* Focused rather than Put back, so Enter cannot lose work. */}
				<button
					type="button"
					className="overwrite__cancel"
					autoFocus
					onClick={() => onAnswer(false)}
				>
					Cancel
				</button>
				<button
					type="button"
					className="overwrite__go"
					onClick={() => onAnswer(true)}
				>
					Put back
				</button>
			</span>
		</div>
	);
}
