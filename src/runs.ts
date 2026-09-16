// Where runs of text come from, for the two places that need them: the editor
// the writer is looking at, and a document on disk that nobody has open.
// Search reads the second and Find reads the first, and they have to agree, so
// both go through the one reading of the tree below.

import { createHeadlessEditor } from "@lexical/headless";
import { $isListItemNode } from "@lexical/list";
import { $findMatchingParent } from "@lexical/utils";
import { $getRoot, type TextNode } from "lexical";
import type { Run } from "./lib/find";
import { $fromMarkdown, EDITOR_NODES } from "./markdown";

/**
 * The block a run belongs to, which is what stops a match running from the end
 * of one into the start of the next. A list's items are the blocks rather than
 * the list itself: every item of a list shares one top-level element, so going
 * by that alone would join `- one` and `- two` into `onetwo`.
 */
function $blockOf(node: TextNode): string {
	const item = $findMatchingParent(node, $isListItemNode);
	return (item ?? node.getTopLevelElementOrThrow()).getKey();
}

/** The runs of text a document is made of, in reading order. */
export function $runs(): Run[] {
	return $getRoot()
		.getAllTextNodes()
		.map((node) => ({
			key: node.getKey(),
			text: node.getTextContent(),
			block: $blockOf(node),
		}));
}

/**
 * The runs of a document nobody has open, by parsing it the way the editor
 * does — the same `$fromMarkdown` and the same node set, so what Search finds
 * is what Find will find once the document is opened.
 *
 * The keys belong to the editor that did the parsing and mean nothing outside
 * it. They are how `matches` says where a match sits, not something to keep.
 */
export function runsOf(text: string): Run[] {
	const editor = createHeadlessEditor({
		namespace: "aurora",
		nodes: EDITOR_NODES,
		onError: (error) => {
			throw error;
		},
	});
	editor.update(() => $fromMarkdown(text), { discrete: true });
	return editor.getEditorState().read(() => $runs());
}
