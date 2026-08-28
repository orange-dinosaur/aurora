import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import { $isHeadingNode } from "@lexical/rich-text";
import {
	$getNodeByKey,
	$getRoot,
	$getSelection,
	$isElementNode,
	$isRangeSelection,
} from "lexical";
import { useEffect, useMemo, useState, type CSSProperties } from "react";
import { containing, outlined, type Block } from "./outline";

/** Long enough that a burst of typing rebuilds the list once, not per key. */
const SETTLE = 200;

type Reading = {
	blocks: Block[];
	/** The block the caret is in, which is what marks the writer's place. */
	at: string | null;
};

function $reading(): Reading {
	const blocks = $getRoot()
		.getChildren()
		.map((node) => ({
			key: node.getKey(),
			level: $isHeadingNode(node) ? Number(node.getTag().slice(1)) : null,
			// Only a heading's own words are ever shown, and reading a whole
			// document's paragraphs to throw them away would not be free.
			text: $isHeadingNode(node) ? node.getTextContent() : "",
		}));

	const selection = $getSelection();
	const at = $isRangeSelection(selection)
		? (selection.anchor.getNode().getTopLevelElement()?.getKey() ?? null)
		: null;

	return { blocks, at };
}

/**
 * The document's headings in a column beside the text. Nothing here writes to
 * the document except the caret it moves when a heading is clicked, which the
 * change plugin ignores.
 */
export default function Outline() {
	const [editor] = useLexicalComposerContext();
	const [reading, setReading] = useState<Reading>(() =>
		editor.getEditorState().read($reading),
	);

	// Rebuilt from the editor state rather than kept in step by hand, so an
	// undo or a document arriving from disk needs no special case.
	useEffect(() => {
		let timer: number | undefined;
		function look() {
			setReading(editor.getEditorState().read($reading));
		}

		const stop = editor.registerUpdateListener(() => {
			window.clearTimeout(timer);
			timer = window.setTimeout(look, SETTLE);
		});
		return () => {
			window.clearTimeout(timer);
			stop();
		};
	}, [editor]);

	const entries = useMemo(() => outlined(reading.blocks), [reading.blocks]);
	const here = containing(reading.blocks, reading.at);

	// The caret goes with the writer, so they can carry straight on from the
	// heading they picked.
	function go(key: string) {
		editor.update(() => {
			const node = $getNodeByKey(key);
			if ($isElementNode(node)) {
				node.selectStart();
			}
		});
		editor.focus();
		editor.getElementByKey(key)?.scrollIntoView({ block: "start" });
	}

	return (
		<nav className="editor__outline" aria-label="Outline">
			{entries.length === 0 ? (
				<p className="editor__outline-none">No headings yet</p>
			) : (
				<ul className="editor__outline-list">
					{entries.map((entry) => (
						<li key={entry.key}>
							<button
								type="button"
								className={
									entry.key === here
										? "editor__outline-item editor__outline-item--here"
										: "editor__outline-item"
								}
								style={
									{ "--depth": entry.depth } as CSSProperties
								}
								aria-current={
									entry.key === here ? "true" : undefined
								}
								title={entry.text}
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
