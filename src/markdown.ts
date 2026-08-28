// Markdown is what sits on disk; a Lexical node tree is what the writer edits.
// Both halves of that conversion live here — the nodes the editor is built
// from, the transformers that carry them across, and the two functions that
// cross the gap — so the editor and the round-trip tests can never disagree
// about what Aurora understands.

import { CodeHighlightNode, CodeNode } from "@lexical/code";
import { AutoLinkNode, LinkNode } from "@lexical/link";
import { ListItemNode, ListNode } from "@lexical/list";
import {
	$convertFromMarkdownString,
	$convertToMarkdownString,
	TRANSFORMERS,
} from "@lexical/markdown";
import { HeadingNode, QuoteNode } from "@lexical/rich-text";
import type { Klass, LexicalNode } from "lexical";

// Nodes and transformers go together: a transformer whose node is not
// registered fails silently, leaving the construct as plain text.
export const EDITOR_NODES: Klass<LexicalNode>[] = [
	HeadingNode,
	QuoteNode,
	ListNode,
	ListItemNode,
	CodeNode,
	CodeHighlightNode,
	LinkNode,
	AutoLinkNode,
];

export const MARKDOWN_TRANSFORMERS = TRANSFORMERS;

/** Replaces the document with the tree a markdown string describes. */
export function $fromMarkdown(text: string): void {
	$convertFromMarkdownString(text, MARKDOWN_TRANSFORMERS);
}

/** The document as markdown, ready to be written to its file. */
export function $toMarkdown(): string {
	return $convertToMarkdownString(MARKDOWN_TRANSFORMERS);
}
