// A screenplay scene in the editor: one script block per line, read from and
// written back to the Fountain text on disk, never through markdown.

import {
	$createTextNode,
	$getRoot,
	$getSelection,
	$getState,
	$isElementNode,
	$isRangeSelection,
	$isTextNode,
	$setState,
	COMMAND_PRIORITY_LOW,
	createState,
	KEY_DOWN_COMMAND,
	ParagraphNode,
	RootNode,
	type EditorConfig,
	type LexicalEditor,
	type LexicalNode,
	type RangeSelection,
} from "lexical";
import { mergeRegister } from "@lexical/utils";
import { split } from "./frontmatter";
import type { Entry } from "./slash";
import type { Filed } from "./subjects";
import {
	classify,
	cueFor,
	CYCLE,
	cycled,
	emphasised,
	follows,
	headingAt,
	overrules,
	parse,
	serialise,
	spans,
	type Block,
	type BlockKind,
	type Span,
	type Style,
} from "./fountain";

const STYLES: Style[] = ["bold", "italic", "underline"];
import { $frontMatter, $setFrontMatter } from "./markdown";

const KINDS: ReadonlySet<string> = new Set<BlockKind>([
	"heading",
	"action",
	"character",
	"dialogue",
	"parenthetical",
	"transition",
	"centered",
	"page-break",
]);

const kind = createState("kind", {
	parse: (value: unknown): BlockKind =>
		typeof value === "string" && KINDS.has(value)
			? (value as BlockKind)
			: "action",
});

const forced = createState("forced", {
	parse: (value: unknown) => value === true,
});

/** A line of the script. A paragraph that knows which block it is. */
export class ScriptNode extends ParagraphNode {
	$config() {
		return this.config("script", {
			extends: ParagraphNode,
			stateConfigs: [
				{ stateConfig: kind, flat: true },
				{ stateConfig: forced, flat: true },
			],
		});
	}

	createDOM(config: EditorConfig): HTMLElement {
		const dom = super.createDOM(config);
		dom.dataset.kind = $getState(this, kind);
		return dom;
	}

	updateDOM(prevNode: this, dom: HTMLElement, config: EditorConfig): boolean {
		dom.dataset.kind = $getState(this, kind);
		return super.updateDOM(prevNode, dom, config);
	}

	// Enter starts the block that usually comes next.
	insertNewAfter(
		selection: RangeSelection,
		restoreSelection: boolean,
	): ParagraphNode {
		const next = super.insertNewAfter(selection, restoreSelection);
		$setState(next, kind, follows($getState(this, kind)));
		return next;
	}
}

// By type rather than class, so a hot reload that swaps the class does not
// leave the open scene's lines unrecognised.
function $isScriptNode(node: LexicalNode): node is ScriptNode {
	return node.getType() === "script";
}

// Every paragraph the editor makes, from Enter to a paste, is a script line.
export const SCRIPT_NODES = [
	ScriptNode,
	{
		replace: ParagraphNode,
		with: () => new ScriptNode(),
		withKlass: ScriptNode,
	},
];

function $line(block: Block): ScriptNode {
	const node = new ScriptNode();
	$setState(node, kind, block.kind);
	$setState(node, forced, block.forced);
	return node.append(
		...spans(block.text).map((span) => {
			const words = $createTextNode(span.text);
			STYLES.forEach((style) => span[style] && words.toggleFormat(style));
			return words;
		}),
	);
}

/** Replaces the document with the blocks a Fountain file holds. */
export function $fromFountain(text: string): void {
	const { block, body } = split(text.replace(/\r\n/g, "\n"));
	const blocks = parse(body);
	$getRoot()
		.clear()
		.append(
			...(blocks.length > 0
				? blocks
				: [{ kind: "action" as const, text: "", forced: false }]
			).map($line),
		);
	$setFrontMatter(block);
}

// The line's words with their emphasis. A link or a line break keeps only its
// text, which is all Fountain has room for.
function $spans(node: LexicalNode): Span[] {
	const pieces = $isElementNode(node) ? node.getChildren() : [node];
	return pieces.map((piece) => ({
		text: piece.getTextContent(),
		bold: $isTextNode(piece) && piece.hasFormat("bold"),
		italic: $isTextNode(piece) && piece.hasFormat("italic"),
		underline: $isTextNode(piece) && piece.hasFormat("underline"),
	}));
}

// Anything else at the top, such as a pasted list, is kept as action so its
// words are not lost on save.
function $block(node: LexicalNode): Block {
	return {
		kind: $isScriptNode(node) ? $getState(node, kind) : "action",
		text: emphasised($spans(node)),
		forced: $isScriptNode(node) && $getState(node, forced),
	};
}

/**
 * The document as Fountain, ready to be written to its file. An empty line
 * has no way to be written, so the one being started is left out until it
 * holds something.
 */
export function $toFountain(): string {
	const head = $frontMatter();
	const body = serialise(
		$getRoot()
			.getChildren()
			.map($block)
			.filter(
				(block) =>
					block.kind === "page-break" || block.text.trim() !== "",
			),
	);
	if (head === "") {
		return body;
	}
	return body === "" ? head : `${head}\n\n${body}`;
}

/** Gives a line the block the writer chose, marking it forced only when its
 * text would have made it something else. */
function $choose(node: ScriptNode, chosen: BlockKind): void {
	const nodes = $getRoot().getChildren().filter($isScriptNode);
	const at = nodes.findIndex((each) => each.is(node));
	$setState(node, forced, overrules(nodes.map($block), at, chosen));
	$setState(node, kind, chosen);
}

const NAMES: Record<string, string> = {
	heading: "Scene heading",
	action: "Action",
	character: "Character",
	dialogue: "Dialogue",
	parenthetical: "Parenthetical",
	transition: "Transition",
};

// Writes `text` where the caret is, then makes its line `chosen`.
function line(
	id: string,
	label: string,
	chosen: BlockKind,
	text = "",
	hint?: string,
): Entry {
	return {
		id,
		label,
		hint,
		run: (editor) =>
			editor.update(() => {
				const selection = $getSelection();
				if (!$isRangeSelection(selection)) {
					return;
				}
				const node = selection.anchor.getNode().getTopLevelElement();
				if (node === null || !$isScriptNode(node)) {
					return;
				}
				if (text !== "") {
					selection.insertText(text);
				}
				$choose(node, chosen);
			}),
	};
}

function cueEntry({ name, folder }: Filed): Entry {
	return {
		...line(
			`cue:${name}`,
			name,
			"character",
			cueFor(name),
			folder || "Character",
		),
		named: true,
	};
}

/** The characters a cue line offers to finish: those whose cue starts with
 * what is typed. Nothing once the line already spells a whole name. */
export function cueEntries(typed: string, characters: Filed[]): Entry[] {
	const wanted = typed.trim().toUpperCase();
	const cues = characters.map((each) => cueFor(each.name));
	if (wanted === "" || cues.includes(wanted)) {
		return [];
	}
	return characters
		.filter((_, at) => cues[at].startsWith(wanted))
		.map(cueEntry);
}

/** What has been typed on a cue line, when the caret stands at its end and the
 * line is plain text. Null anywhere else. */
export function $cueAt(): string | null {
	const selection = $getSelection();
	if (
		!$isRangeSelection(selection) ||
		!selection.isCollapsed() ||
		selection.anchor.type !== "text"
	) {
		return null;
	}
	const node = selection.anchor.getNode();
	const owner = node.getTopLevelElement();
	if (
		owner === null ||
		!$isScriptNode(owner) ||
		$getState(owner, kind) !== "character"
	) {
		return null;
	}
	const typed = node.getTextContent().slice(0, selection.anchor.offset);
	return typed === owner.getTextContent() && typed.trim() !== ""
		? typed
		: null;
}

/** What a slash offers in a script: the blocks, then each character as a cue
 * and each location as the start of a scene heading. */
export function scriptEntries(
	characters: Filed[],
	locations: Filed[],
): Entry[] {
	return [
		...CYCLE.map((each) => line(each, NAMES[each], each)),
		...characters.map(cueEntry),
		...locations.map(({ name, folder }) => ({
			...line(
				`place:${name}`,
				name,
				"heading",
				headingAt(name),
				folder || "Location",
			),
			named: true,
		})),
	];
}

/**
 * Works each line's block out again whenever the text changes. The root's
 * transform runs last in every update that changed anything, so the kinds are
 * settled inside the same update and undo takes them back with the text.
 */
export function registerScript(editor: LexicalEditor): () => void {
	return mergeRegister(
		editor.registerNodeTransform(RootNode, () => {
			const nodes = $getRoot().getChildren().filter($isScriptNode);
			const kinds = classify(nodes.map($block));
			nodes.forEach((node, i) => {
				if ($getState(node, kind) !== kinds[i]) {
					$setState(node, kind, kinds[i]);
				}
			});
		}),
		// Tab changes the block the caret is in when the guess was wrong. Matched
		// by the physical key, because Linux names Shift+Tab "ISO_Left_Tab" and
		// Lexical's own Tab command only answers to "Tab".
		editor.registerCommand(
			KEY_DOWN_COMMAND,
			(event) => {
				if (
					(event.code !== "Tab" && event.key !== "Tab") ||
					event.ctrlKey ||
					event.altKey ||
					event.metaKey
				) {
					return false;
				}
				const selection = $getSelection();
				if (!$isRangeSelection(selection)) {
					return false;
				}
				const node = selection.anchor.getNode().getTopLevelElement();
				if (node === null || !$isScriptNode(node)) {
					return false;
				}
				event.preventDefault();
				$choose(
					node,
					cycled($getState(node, kind), event.shiftKey ? -1 : 1),
				);
				return true;
			},
			// Above Lexical's own keydown handler, which runs at the editor's
			// priority and takes every key it recognises.
			COMMAND_PRIORITY_LOW,
		),
	);
}
