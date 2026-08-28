// Everything Aurora can do to a piece of text, declared once. The bar that
// floats over a selection reads from here, and so will the bar under the title
// and the slash menu — so none of them can offer different things, or disagree
// about what the caret is already inside.

import {
	$createLinkNode,
	$isLinkNode,
	$toggleLink,
	formatUrl,
} from "@lexical/link";
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
import { $findMatchingParent } from "@lexical/utils";
import {
	$createParagraphNode,
	$createTextNode,
	$getSelection,
	$isParagraphNode,
	$isRangeSelection,
	FORMAT_TEXT_COMMAND,
	IS_APPLE,
	type ElementNode,
	type LexicalCommand,
	type LexicalEditor,
	type LexicalNode,
	type RangeSelection,
	type TextFormatType,
} from "lexical";

// The command key — ⌘ on a Mac, Ctrl everywhere else — is always part of a
// shortcut, so only what is held with it is worth recording.
export type Keys = { key: string; shift: boolean };

export type Action = {
	id: string;
	label: string;
	keys: Keys;
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
	keys: Keys,
): Mark {
	return {
		id,
		label,
		glyph,
		keys,
		isActive: (selection) => selection.hasFormat(format),
		run: (editor) => editor.dispatchCommand(FORMAT_TEXT_COMMAND, format),
	};
}

/** What can be put on the words themselves. */
export const MARKS: Mark[] = [
	mark("bold", "Bold", "B", "bold", { key: "b", shift: false }),
	mark("italic", "Italic", "I", "italic", { key: "i", shift: false }),
	mark("strikethrough", "Strikethrough", "S", "strikethrough", {
		key: "s",
		shift: true,
	}),
	mark("highlight", "Highlight", "▨", "highlight", {
		key: "h",
		shift: true,
	}),
	mark("code", "Code", "<>", "code", { key: "e", shift: false }),
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
	digit: string,
	matches: (node: LexicalNode) => boolean,
	run: (editor: LexicalEditor) => void,
): Action {
	return {
		id,
		label,
		keys: { key: digit, shift: true },
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

function heading(tag: HeadingTagType, label: string, digit: string): Action {
	return block(
		tag,
		label,
		digit,
		(node) => $isHeadingNode(node) && node.getTag() === tag,
		turnInto(() => $createHeadingNode(tag)),
	);
}

function list(
	kind: "bullet" | "number",
	label: string,
	digit: string,
	command: LexicalCommand<void>,
): Action {
	return block(
		kind,
		label,
		digit,
		(node) => $isListNode(node) && node.getListType() === kind,
		(editor) => editor.dispatchCommand(command, undefined),
	);
}

/** What a whole paragraph can be turned into. */
export const BLOCKS: Action[] = [
	block(
		"paragraph",
		"Text",
		"0",
		$isParagraphNode,
		turnInto($createParagraphNode),
	),
	heading("h1", "Heading 1", "1"),
	heading("h2", "Heading 2", "2"),
	heading("h3", "Heading 3", "3"),
	block("quote", "Quote", "4", $isQuoteNode, turnInto($createQuoteNode)),
	list("bullet", "Bulleted list", "5", INSERT_UNORDERED_LIST_COMMAND),
	list("number", "Numbered list", "6", INSERT_ORDERED_LIST_COMMAND),
];

/** Everything a key press could be asking for, marks first. */
export const ACTIONS: Action[] = [...MARKS, ...BLOCKS];

/** Whether a key press is asking for this shortcut, and nothing else. */
export function pressed(event: KeyboardEvent, keys: Keys): boolean {
	const command = IS_APPLE ? event.metaKey : event.ctrlKey;
	const wrong = IS_APPLE ? event.ctrlKey : event.metaKey;
	if (!command || wrong || event.altKey || event.shiftKey !== keys.shift) {
		return false;
	}
	// A shifted digit arrives as its symbol — Shift+1 is "!" — so the key on
	// the keyboard is the only thing that identifies it. Letters are matched
	// the other way round, by what they type, so a remapped layout still works.
	return (
		event.key.toLowerCase() === keys.key ||
		(/^[0-9]$/.test(keys.key) && event.code === `Digit${keys.key}`)
	);
}

const COMMAND_KEY = IS_APPLE ? "⌘" : "Ctrl+";
const SHIFT_KEY = IS_APPLE ? "⇧" : "Shift+";

/** The shortcut written out, for a menu or a tooltip. */
export function shortcutLabel(keys: Keys): string {
	return `${COMMAND_KEY}${keys.shift ? SHIFT_KEY : ""}${keys.key.toUpperCase()}`;
}

/** The block the selection sits in, or null when it spans more than one kind. */
export function $blockOf(selection: RangeSelection): Action | null {
	return BLOCKS.find((action) => action.isActive(selection)) ?? null;
}

// A link is not a mark: it needs an address, so the bar has to ask for one
// rather than just being pressed. What is shared with the rest of the
// vocabulary is the reading — whether the caret is inside one, and where it
// points.

/** The address of the link the selection is inside, or null. */
export function $linkAt(selection: RangeSelection): string | null {
	const link = $findMatchingParent(selection.anchor.getNode(), $isLinkNode);
	return $isLinkNode(link) ? link.getURL() : null;
}

/** Puts an address on the selection, or takes the link off it when null. */
export function setLink(editor: LexicalEditor, url: string | null): void {
	editor.update(() => {
		const selection = $getSelection();
		if (
			url !== null &&
			$isRangeSelection(selection) &&
			selection.isCollapsed() &&
			$linkAt(selection) === null
		) {
			// Nothing is selected, so the address becomes its own words. Left
			// to itself the toggle would swallow whatever the caret happened
			// to be sitting in.
			const link = $createLinkNode(url);
			link.append($createTextNode(url));
			selection.insertNodes([link]);
			return;
		}
		$toggleLink(url);
	});
}

// What the desktop is allowed to be handed. The Tauri opener's own scope stops
// at these, so anything else is refused there rather than here — asking first
// means a link that will not open never looks like one that failed to.
const OPENABLE = /^(?:https?|mailto|tel):/i;

/** Whether Aurora may ask the desktop to open this address. */
export function openable(url: string): boolean {
	return OPENABLE.test(url.trim());
}

/** What the writer typed, made into an address. Lexical's own tidying: a bare
 * domain gains https://, something with an @ becomes mailto:, and a relative
 * path is left as the writer meant it. */
export function linkTarget(typed: string): string {
	const url = typed.trim();
	return url === "" ? "" : formatUrl(url);
}

/** A reading of the selection, for a bar to draw itself from. */
export type Formatting = {
	marks: ReadonlySet<string>;
	block: Action | null;
	link: string | null;
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
		a.link === b.link &&
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
		link: $linkAt(selection),
	};
}
