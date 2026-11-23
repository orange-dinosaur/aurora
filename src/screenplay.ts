// A screenplay scene in the editor: one script block per line, read from and
// written back to the Fountain text on disk, never through markdown.

import {
	$createTextNode,
	$getRoot,
	$getState,
	$setState,
	createState,
	ParagraphNode,
	RootNode,
	type EditorConfig,
	type LexicalEditor,
	type LexicalNode,
} from "lexical";
import { split } from "./frontmatter";
import {
	classify,
	parse,
	serialise,
	type Block,
	type BlockKind,
} from "./fountain";
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
}

function $isScriptNode(node: LexicalNode): node is ScriptNode {
	return node instanceof ScriptNode;
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
	return block.text === "" ? node : node.append($createTextNode(block.text));
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

// Anything else at the top, such as a heading the toolbar made, is kept as
// action so its words are not lost on save.
function $block(node: LexicalNode): Block {
	return {
		kind: $isScriptNode(node) ? $getState(node, kind) : "action",
		text: node.getTextContent(),
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

/**
 * Works each line's block out again whenever the text changes. The root's
 * transform runs last in every update that changed anything, so the kinds are
 * settled inside the same update and undo takes them back with the text.
 */
export function registerScript(editor: LexicalEditor): () => void {
	return editor.registerNodeTransform(RootNode, () => {
		const nodes = $getRoot().getChildren().filter($isScriptNode);
		const kinds = classify(nodes.map($block));
		nodes.forEach((node, i) => {
			if ($getState(node, kind) !== kinds[i]) {
				$setState(node, kind, kinds[i]);
			}
		});
	});
}
