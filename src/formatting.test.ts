import { createHeadlessEditor } from "@lexical/headless";
import { registerList } from "@lexical/list";
import { registerRichText } from "@lexical/rich-text";
import { $getRoot, $getSelection, $isRangeSelection } from "lexical";
import { beforeEach, describe, expect, test } from "vitest";
import { $blockOf, BLOCKS, MARKS, type Action } from "./formatting";
import { $fromMarkdown, $toMarkdown, EDITOR_NODES } from "./markdown";

// The list and heading commands are handled by plugins, not by the editor
// itself, so a headless editor has to register them to answer for the actions
// that dispatch them.
function editorWith(markdown: string) {
	const editor = createHeadlessEditor({
		namespace: "aurora",
		nodes: EDITOR_NODES,
		onError: (error) => {
			throw error;
		},
	});
	registerRichText(editor);
	registerList(editor);
	editor.update(
		() => {
			$fromMarkdown(markdown);
			$getRoot().selectStart();
		},
		{ discrete: true },
	);
	return editor;
}

// An action commits its change on Lexical's own schedule, so a test that wants
// to read the result has to ask for a commit first.
function markdownOf(editor: ReturnType<typeof editorWith>): string {
	editor.update(() => {}, { discrete: true });
	return editor.getEditorState().read(() => $toMarkdown());
}

function find(id: string): Action {
	const action = BLOCKS.find((each) => each.id === id);
	if (action === undefined) {
		throw new Error(`no block action ${id}`);
	}
	return action;
}

describe("turning a paragraph into another kind of block", () => {
	test.each([
		["h1", "# She ran."],
		["h2", "## She ran."],
		["h3", "### She ran."],
		["quote", "> She ran."],
		["bullet", "- She ran."],
		["number", "1. She ran."],
	])("%s writes itself as markdown", (id, expected) => {
		const editor = editorWith("She ran.");
		find(id).run(editor);
		expect(markdownOf(editor)).toBe(expected);
	});
});

describe("turning a block back into plain text", () => {
	test.each([
		["a heading", "# She ran."],
		["a quote", "> She ran."],
		["a bulleted list", "- She ran."],
		["a numbered list", "1. She ran."],
	])("%s becomes a paragraph again", (_what, markdown) => {
		const editor = editorWith(markdown);
		find("paragraph").run(editor);
		expect(markdownOf(editor)).toBe("She ran.");
	});
});

describe("what the selection reports it is inside", () => {
	function blockIdOf(editor: ReturnType<typeof editorWith>): string | null {
		return editor.getEditorState().read(() => {
			const selection = $getSelection();
			if (!$isRangeSelection(selection)) {
				return null;
			}
			return $blockOf(selection)?.id ?? null;
		});
	}

	test.each([
		["a paragraph", "She ran.", "paragraph"],
		["a heading", "## She ran.", "h2"],
		["a quote", "> She ran.", "quote"],
		["a bulleted list", "- She ran.", "bullet"],
		["a numbered list", "1. She ran.", "number"],
	])("in %s it says %s", (_what, markdown, id) => {
		expect(blockIdOf(editorWith(markdown))).toBe(id);
	});

	test("a selection across two kinds of block reports neither", () => {
		const editor = editorWith("# Chapter One\n\nShe ran.");
		editor.update(
			() => {
				$getRoot().select(0, $getRoot().getChildrenSize());
			},
			{ discrete: true },
		);
		expect(blockIdOf(editor)).toBeNull();
	});

	test("a scene break is not a block anything can be turned into", () => {
		expect(blockIdOf(editorWith("---"))).toBeNull();
	});
});

describe("the marks on words", () => {
	let editor: ReturnType<typeof editorWith>;

	beforeEach(() => {
		editor = editorWith("She ran.");
		editor.update(
			() => {
				$getRoot().getFirstChild()?.selectStart();
				const selection = $getSelection();
				if ($isRangeSelection(selection)) {
					selection.focus.offset = 3;
				}
			},
			{ discrete: true },
		);
	});

	test.each([
		["bold", "**She** ran."],
		["italic", "*She* ran."],
		["strikethrough", "~~She~~ ran."],
		["highlight", "==She== ran."],
		["code", "`She` ran."],
	])("%s writes itself as markdown", (id, expected) => {
		const action = MARKS.find((each) => each.id === id);
		action?.run(editor);
		expect(markdownOf(editor)).toBe(expected);
	});

	test("an unmarked selection reports no marks", () => {
		const active = editor.getEditorState().read(() => {
			const selection = $getSelection();
			return $isRangeSelection(selection)
				? MARKS.filter((each) => each.isActive(selection)).length
				: -1;
		});
		expect(active).toBe(0);
	});
});
