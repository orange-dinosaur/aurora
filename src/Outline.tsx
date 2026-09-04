import type { CSSProperties } from "react";
import type { OutlineHandle } from "./outline";

// The open document's headings, listed under its synopsis. Both the reading
// and the jump come from the editor the document is open in; this only draws
// them.

export default function Outline({ entries, here, go }: OutlineHandle) {
	return (
		<nav className="outline" aria-label="Outline">
			{entries.length === 0 ? (
				<p className="outline__none">No headings yet</p>
			) : (
				<ul className="outline__list">
					{entries.map((entry) => (
						<li key={entry.key}>
							<button
								type="button"
								className={
									entry.key === here
										? "outline__item outline__item--here"
										: "outline__item"
								}
								style={
									{ "--depth": entry.depth } as CSSProperties
								}
								aria-current={
									entry.key === here ? "true" : undefined
								}
								onClick={() => go(entry.key)}
							>
								{entry.text.trim() === ""
									? "Untitled"
									: entry.text}
							</button>
						</li>
					))}
				</ul>
			)}
		</nav>
	);
}
