// Blocks drawn the way the editor draws them, for pages that only show text.

import type { ReactNode } from "react";
import type { Block, Run } from "./lib/compare";

/** The class of the wash over a difference. */
export type Look = (difference: number) => string;

export function draw(block: Block, key: number, look: Look): ReactNode {
	switch (block.kind) {
		case "break":
			return <hr key={key} className="editor__rule" />;
		case "heading": {
			const Heading = `h${block.level}` as "h1";
			return (
				<Heading
					key={key}
					className={`editor__heading editor__heading--${block.level}`}
				>
					{block.runs.map((run, index) => drawRun(run, index, look))}
				</Heading>
			);
		}
		case "quote":
			return (
				<blockquote key={key} className="editor__quote">
					{block.runs.map((run, index) => drawRun(run, index, look))}
				</blockquote>
			);
		case "item":
		case "paragraph":
			return (
				<p
					key={key}
					className={
						block.kind === "item"
							? "editor__paragraph compare__item"
							: "editor__paragraph"
					}
				>
					{block.runs.map((run, index) => drawRun(run, index, look))}
				</p>
			);
	}
}

function drawRun(run: Run, key: number, look: Look): ReactNode {
	// An empty run stays empty, so its mark draws as a caret.
	let drawn: ReactNode = run.text === "" ? null : run.text;
	if (run.em && drawn !== null) {
		drawn = <em>{drawn}</em>;
	}
	if (run.strong && drawn !== null) {
		drawn = <strong>{drawn}</strong>;
	}
	return run.difference === null ? (
		<span key={key}>{drawn}</span>
	) : (
		<Wash key={key} difference={run.difference} look={look}>
			{drawn}
		</Wash>
	);
}

export function Wash({
	difference,
	look,
	children,
}: {
	difference: number;
	look: Look;
	children: ReactNode;
}) {
	return (
		<mark className={look(difference)} data-difference={difference}>
			{children}
		</mark>
	);
}
