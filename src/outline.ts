import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import { $isHeadingNode } from "@lexical/rich-text";
import {
	$getNodeByKey,
	$getRoot,
	$getSelection,
	$isElementNode,
	$isRangeSelection,
} from "lexical";
import { useCallback, useEffect, useMemo, useState } from "react";

// The outline is worked out from a copy of the document's top-level blocks,
// the same way find works from a copy of its runs of text: the hook reads them
// out of the editor, this turns them into a list, and nothing here can touch a
// node.

/** A top-level block, as the outline sees it. */
export type Block = {
	key: string;
	/** 1 to 6 for a heading, null for anything that is not one. */
	level: number | null;
	text: string;
};

/** A heading in the list, with the indent it is drawn at. */
export type Entry = {
	key: string;
	level: number;
	text: string;
	/** Counted in steps of indentation, not in heading levels. */
	depth: number;
};

/**
 * The headings in reading order. Depth comes from the levels the document
 * actually uses rather than from the level itself, so a chapter written
 * entirely in Heading 2, or one that jumps from 1 straight to 3, still reads as
 * a list instead of as a ladder with rungs missing.
 */
export function outlined(blocks: Block[]): Entry[] {
	const entries: Entry[] = [];
	// The levels of the headings this one is still inside.
	const open: number[] = [];

	for (const block of blocks) {
		if (block.level === null) {
			continue;
		}

		// Anything at this level or below it is a sibling or an uncle, not a
		// parent, so it stops counting towards the indent.
		while (open.length > 0 && open[open.length - 1] >= block.level) {
			open.pop();
		}

		entries.push({
			key: block.key,
			level: block.level,
			text: block.text,
			depth: open.length,
		});
		open.push(block.level);
	}

	return entries;
}

/**
 * The heading whose section the given block sits in, which is what marks where
 * the writer is. Null while the caret is above the first heading, or when the
 * block is not one of these.
 */
export function containing(blocks: Block[], at: string | null): string | null {
	if (at === null) {
		return null;
	}

	let heading: string | null = null;
	for (const block of blocks) {
		if (block.level !== null) {
			heading = block.key;
		}
		if (block.key === at) {
			return heading;
		}
	}

	return null;
}

/** Long enough that a burst of typing rebuilds the list once, not per key. */
const SETTLE = 200;

/** The outline as the sidebar draws it, with the way back into the editor. */
export type OutlineHandle = {
	entries: Entry[];
	/** The heading whose section the caret is in, so the list says where the
	 * writer is as well as where they could go. */
	here: string | null;
	go: (key: string) => void;
};

type Reading = {
	blocks: Block[];
	/** The block the caret is in. */
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

// Held onto so that writing a paragraph, which changes no heading and moves
// the caret nowhere new, hands back the reading already in state. The panel
// sits above this in the tree, so a new object here would render the project.
function same(a: Reading, b: Reading): boolean {
	return (
		a.at === b.at &&
		a.blocks.length === b.blocks.length &&
		a.blocks.every((block, i) => {
			const other = b.blocks[i];
			return (
				block.key === other.key &&
				block.level === other.level &&
				block.text === other.text
			);
		})
	);
}

/**
 * The open document's headings, read from the editor this is mounted in.
 * Nothing here writes to the document except the caret it moves when a heading
 * is picked, which the change plugin ignores.
 */
export function useOutline(): OutlineHandle {
	const [editor] = useLexicalComposerContext();
	const [reading, setReading] = useState<Reading>(() =>
		editor.getEditorState().read($reading),
	);

	// Rebuilt from the editor state rather than kept in step by hand, so an
	// undo or a document arriving from disk needs no special case.
	useEffect(() => {
		let timer: number | undefined;
		function look() {
			const next = editor.getEditorState().read($reading);
			setReading((was) => (same(was, next) ? was : next));
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
	const go = useCallback(
		(key: string) => {
			editor.update(() => {
				const node = $getNodeByKey(key);
				if ($isElementNode(node)) {
					node.selectStart();
				}
			});
			editor.focus();
			editor.getElementByKey(key)?.scrollIntoView({ block: "start" });
		},
		[editor],
	);

	return useMemo(() => ({ entries, here, go }), [entries, here, go]);
}
