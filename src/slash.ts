// The slash menu's own reading of the text: when a slash is asking for a menu,
// and which of the blocks answer to what has been typed after it. Kept apart
// from the component so it can be tested without a browser.

import { $findMatchingParent } from "@lexical/utils";
import {
	$getSelection,
	$isElementNode,
	$isRangeSelection,
	type LexicalNode,
} from "lexical";
import { BLOCKS, INSERTS, type Action } from "./formatting";

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
	if (block === null) {
		return null;
	}
	return slashQuery(
		node.getTextContent().slice(0, anchor.offset),
		block.getTextContent(),
	);
}

// What a paragraph can become, and then what can be dropped in after it.
const OFFERED: Action[] = [...BLOCKS, ...INSERTS];

/** The entries whose names answer to what has been typed. Any word of a name
 * can be the one that matches, so "list" finds both of them. */
export function matching(query: string): Action[] {
	const wanted = query.trim().toLowerCase();
	if (wanted === "") {
		return OFFERED;
	}
	return OFFERED.filter((action) =>
		action.label
			.toLowerCase()
			.split(" ")
			.some((word) => word.startsWith(wanted)),
	);
}
