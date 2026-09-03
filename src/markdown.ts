// Markdown is what sits on disk; a Lexical node tree is what the writer edits.
// Both halves of that conversion live here — the nodes the editor is built
// from, the transformers that carry them across, and the two functions that
// cross the gap — so the editor and the round-trip tests can never disagree
// about what Aurora understands.

import { CodeHighlightNode, CodeNode } from "@lexical/code";
import {
	$createHorizontalRuleNode,
	$isHorizontalRuleNode,
	HorizontalRuleNode,
} from "@lexical/extension";
import { AutoLinkNode, LinkNode } from "@lexical/link";
import { ListItemNode, ListNode } from "@lexical/list";
import {
	$convertFromMarkdownString,
	$convertToMarkdownString,
	TRANSFORMERS,
	type ElementTransformer,
} from "@lexical/markdown";
import { HeadingNode, QuoteNode } from "@lexical/rich-text";
import {
	$getRoot,
	$getState,
	$setState,
	createState,
	type Klass,
	type LexicalNode,
} from "lexical";
import { split } from "./frontmatter";

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
	HorizontalRuleNode,
];

// Every spelling of a thematic break a writer might use, including the spaced
// asterisks a novel marks a scene change with. Without this the asterisks read
// as a bullet and the break becomes a list item.
const SCENE_BREAK: ElementTransformer = {
	dependencies: [HorizontalRuleNode],
	export: (node) => ($isHorizontalRuleNode(node) ? "---" : null),
	regExp: /^(?:-\s*){3,}$|^(?:\*\s*){3,}$|^(?:_\s*){3,}$/,
	replace: (parentNode, _children, _match, isImport) => {
		const rule = $createHorizontalRuleNode();
		if (isImport || parentNode.getNextSibling() !== null) {
			parentNode.replace(rule);
		} else {
			parentNode.insertBefore(rule);
		}
		rule.selectNext();
	},
	type: "element",
};

export const MARKDOWN_TRANSFORMERS = [SCENE_BREAK, ...TRANSFORMERS];

// The front matter block a file arrived with, kept as it was written and put
// back on the way out. `frontmatter.ts` owns what it means; here it is only
// text to be carried.
const frontMatter = createState("frontMatter", {
	parse: (value: unknown) => (typeof value === "string" ? value : ""),
});

/** The block the open document carries, fences and all, or "" if it has none. */
export function $frontMatter(): string {
	return $getState($getRoot(), frontMatter);
}

/** Replaces that block. An empty string leaves the document without one. */
export function $setFrontMatter(block: string): void {
	$setState($getRoot(), frontMatter, block);
}

/** Replaces the document with the tree a markdown string describes. */
export function $fromMarkdown(text: string): void {
	const { block, body } = split(text);
	$convertFromMarkdownString(body.replace(/^\n+/, ""), MARKDOWN_TRANSFORMERS);
	$setFrontMatter(block);
}

/** The document as markdown, ready to be written to its file. */
export function $toMarkdown(): string {
	const head = $frontMatter();
	const body = $convertToMarkdownString(MARKDOWN_TRANSFORMERS);
	if (head === "") {
		return body;
	}
	return body === "" ? head : `${head}\n\n${body}`;
}
