// Everything Aurora can do to a piece of text, declared once. The bar that
// floats over a selection reads from here, and so will the bar under the title
// and the slash menu — so none of them can offer different things, or disagree
// about what the caret is already inside.

import {
	$isListNode,
	INSERT_ORDERED_LIST_COMMAND,
	INSERT_UNORDERED_LIST_COMMAND,
} from "@lexical/list";
import {
	$createHeadingNode,
	$createQuoteNode,
	$isHeadingNode,
	$isQuoteNode,
	type HeadingTagType,
} from "@lexical/rich-text";
import { $setBlocksType } from "@lexical/selection";
import {
	$createParagraphNode,
	$getSelection,
	$isParagraphNode,
	$isRangeSelection,
	FORMAT_TEXT_COMMAND,
	type ElementNode,
	type LexicalCommand,
	type LexicalEditor,
	type LexicalNode,
	type RangeSelection,
	type TextFormatType,
} from "lexical";

export type Action = {
	id: string;
	label: string;
	// Callable only inside a read or an update, hence the naming Lexical uses.
	isActive: (selection: RangeSelection) => boolean;
	run: (editor: LexicalEditor) => void;
};

/** An action drawn as a single character rather than named. */
export type Mark = Action & { glyph: string };

function mark(
	id: string,
	label: string,
	glyph: string,
	format: TextFormatType,
): Mark {
	return {
		id,
		label,
		glyph,
		isActive: (selection) => selection.hasFormat(format),
		run: (editor) => editor.dispatchCommand(FORMAT_TEXT_COMMAND, format),
	};
}

/** What can be put on the words themselves. */
export const MARKS: Mark[] = [
	mark("bold", "Bold", "B", "bold"),
	mark("italic", "Italic", "I", "italic"),
	mark("strikethrough", "Strikethrough", "S", "strikethrough"),
	mark("highlight", "Highlight", "▨", "highlight"),
	mark("code", "Code", "<>", "code"),
];

// The blocks a selection reaches into, named as a writer would name them: a
// list item's block is the list it belongs to, not the item.
function $blocksOf(selection: RangeSelection): LexicalNode[] {
	const found = new Map<string, LexicalNode>();
	for (const node of [selection.anchor.getNode(), ...selection.getNodes()]) {
		const block = node.getTopLevelElement();
		if (block !== null) {
			found.set(block.getKey(), block);
		}
	}
	return [...found.values()];
}

function block(
	id: string,
	label: string,
	matches: (node: LexicalNode) => boolean,
	run: (editor: LexicalEditor) => void,
): Action {
	return {
		id,
		label,
		isActive: (selection) => {
			const blocks = $blocksOf(selection);
			return blocks.length > 0 && blocks.every(matches);
		},
		run,
	};
}

function turnInto(create: () => ElementNode) {
	return (editor: LexicalEditor) =>
		editor.update(() => {
			const selection = $getSelection();
			if ($isRangeSelection(selection)) {
				$setBlocksType(selection, create);
			}
		});
}

function heading(tag: HeadingTagType, label: string): Action {
	return block(
		tag,
		label,
		(node) => $isHeadingNode(node) && node.getTag() === tag,
		turnInto(() => $createHeadingNode(tag)),
	);
}

function list(
	kind: "bullet" | "number",
	label: string,
	command: LexicalCommand<void>,
): Action {
	return block(
		kind,
		label,
		(node) => $isListNode(node) && node.getListType() === kind,
		(editor) => editor.dispatchCommand(command, undefined),
	);
}

/** What a whole paragraph can be turned into. */
export const BLOCKS: Action[] = [
	block(
		"paragraph",
		"Text",
		$isParagraphNode,
		turnInto($createParagraphNode),
	),
	heading("h1", "Heading 1"),
	heading("h2", "Heading 2"),
	heading("h3", "Heading 3"),
	block("quote", "Quote", $isQuoteNode, turnInto($createQuoteNode)),
	list("bullet", "Bulleted list", INSERT_UNORDERED_LIST_COMMAND),
	list("number", "Numbered list", INSERT_ORDERED_LIST_COMMAND),
];

/** The block the selection sits in, or null when it spans more than one kind. */
export function $blockOf(selection: RangeSelection): Action | null {
	return BLOCKS.find((action) => action.isActive(selection)) ?? null;
}

/** A reading of the selection, for a bar to draw itself from. */
export type Formatting = {
	marks: ReadonlySet<string>;
	block: Action | null;
};

/** Whether two readings would draw the same bar. */
export function sameFormatting(
	a: Formatting | null,
	b: Formatting | null,
): boolean {
	if (a === null || b === null) {
		return a === b;
	}
	// The actions are declared once and never rebuilt, so identity is enough.
	return (
		a.block === b.block &&
		a.marks.size === b.marks.size &&
		[...a.marks].every((id) => b.marks.has(id))
	);
}

export function $formattingOf(selection: RangeSelection): Formatting {
	return {
		marks: new Set(
			MARKS.filter((action) => action.isActive(selection)).map(
				(action) => action.id,
			),
		),
		block: $blockOf(selection),
	};
}
