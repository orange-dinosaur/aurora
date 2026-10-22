// The question before a put back that would write over changes not in a
// checkpoint. It floats over the window, so asking never moves what is under it.

import { useEffect, useRef, useState } from "react";
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

	// Watched on the document, as a menu is: a click anywhere else cancels.
	useEffect(() => {
		function away(event: MouseEvent) {
			const at = event.target;
			if (!(at instanceof Node) || !box.current?.contains(at)) {
				onAnswer(false);
			}
		}
		function escape(event: KeyboardEvent) {
			if (event.key === "Escape") {
				onAnswer(false);
			}
		}
		document.addEventListener("mousedown", away);
		document.addEventListener("keydown", escape);
		return () => {
			document.removeEventListener("mousedown", away);
			document.removeEventListener("keydown", escape);
		};
	}, [onAnswer]);

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
					className="sprint__stop"
					autoFocus
					onClick={() => onAnswer(false)}
				>
					Cancel
				</button>
				<button
					type="button"
					className="sprint__go"
					onClick={() => onAnswer(true)}
				>
					Put back
				</button>
			</span>
		</div>
	);
}
