// The slash menu's own reading of the text: when a slash is asking for a menu,
// and which of the blocks answer to what has been typed after it. Kept apart
// from the component so it can be tested without a browser.

import { $isCodeNode } from "@lexical/code";
import { $findMatchingParent } from "@lexical/utils";
import {
	$getSelection,
	$isElementNode,
	$isRangeSelection,
	type LexicalNode,
} from "lexical";
import { BLOCKS, INSERTS, type Action, type Keys } from "./formatting";

// Past this the slash is not a search any more, it is a sentence that happens
// to start with one.
const LONGEST = 24;

/** What has been typed after a slash that stands alone in its block, given the
 * text up to the caret and the whole block's text. Null when it is anything
 * else — a slash mid-sentence, or one with words already after it. */
export function slashQuery(upToCaret: string, block: string): string | null {
	if (upToCaret !== block || !upToCaret.startsWith("/")) {
		return null;
	}
	const query = upToCaret.slice(1);
	return query.length > LONGEST ? null : query;
}

function isBlock(node: LexicalNode): boolean {
	return $isElementNode(node) && !node.isInline();
}

/** The same reading, taken from where the caret is now. */
export function $slashAt(): string | null {
	const selection = $getSelection();
	if (!$isRangeSelection(selection) || !selection.isCollapsed()) {
		return null;
	}
	const anchor = selection.anchor;
	// An element anchor means the block holds no text yet, so there is no
	// slash in it to find.
	if (anchor.type !== "text") {
		return null;
	}
	const node = anchor.getNode();
	const block = $findMatchingParent(node, isBlock);
	if (block === null || $isCodeNode(block)) {
		return null;
	}
	return slashQuery(
		node.getTextContent().slice(0, anchor.offset),
		block.getTextContent(),
	);
}

/** One line of the menu. A shortcut is shown when it has one, and otherwise a
 * word saying what it is. */
export type Entry = Pick<Action, "id" | "label" | "run"> & {
	keys?: Keys;
	hint?: string;
	/** Offered only once something is typed, so a long cast never fills a
	 * bare slash's menu. */
	named?: boolean;
};

/** What a paragraph can become, and then what can be dropped in after it. */
export const NOVEL: Entry[] = [...BLOCKS, ...INSERTS];

/** The entries whose names answer to what has been typed. Any word of a name
 * can be the one that matches, so "list" finds both of them. */
export function matching(query: string, offered: Entry[]): Entry[] {
	const wanted = query.trim().toLowerCase();
	if (wanted === "") {
		return offered.filter((entry) => !entry.named);
	}
	// Any run of the label's words can start with the query, so a two-word
	// query like "heading 1" finds Heading 1.
	return offered.filter((action) => {
		const words = action.label.toLowerCase().split(" ");
		return words.some((_, at) =>
			words.slice(at).join(" ").startsWith(wanted),
		);
	});
}
