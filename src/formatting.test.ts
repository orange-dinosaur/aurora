import { createHeadlessEditor } from "@lexical/headless";
import { registerList } from "@lexical/list";
import { registerRichText } from "@lexical/rich-text";
import { $getRoot, $getSelection, $isRangeSelection } from "lexical";
import { beforeEach, describe, expect, test } from "vitest";
import {
	$blockOf,
	$formattingOf,
	$linkAt,
	ACTIONS,
	BLOCKS,
	MARKS,
	linkTarget,
	openable,
	pressed,
	sameFormatting,
	setLink,
	shortcutLabel,
	type Action,
	type Keys,
} from "./formatting";
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

// A bar only redraws itself when this says the reading changed, so a false
// positive here would freeze it on a stale answer.
describe("comparing two readings of the selection", () => {
	function readingOf(markdown: string) {
		const editor = editorWith(markdown);
		return editor.getEditorState().read(() => {
			const selection = $getSelection();
			if (!$isRangeSelection(selection)) {
				throw new Error("no selection");
			}
			return $formattingOf(selection);
		});
	}

	test("two readings of the same block are the same", () => {
		expect(
			sameFormatting(readingOf("She ran."), readingOf("He sat.")),
		).toBe(true);
	});

	test("different blocks are not the same", () => {
		expect(
			sameFormatting(readingOf("She ran."), readingOf("# Chapter")),
		).toBe(false);
	});

	test("nothing selected matches only nothing selected", () => {
		expect(sameFormatting(null, null)).toBe(true);
		expect(sameFormatting(null, readingOf("She ran."))).toBe(false);
	});

	test("the same block with a different mark is not the same", () => {
		const plain = readingOf("She ran.");
		expect(
			sameFormatting(plain, {
				block: plain.block,
				marks: new Set(["bold"]),
				link: null,
			}),
		).toBe(false);
	});
});

// Only the fields the matcher looks at. The tests run as they would on this
// machine, where the command key is Ctrl.
function press(
	key: string,
	held: {
		shift?: boolean;
		ctrl?: boolean;
		alt?: boolean;
		meta?: boolean;
	} = {},
	code = "",
): KeyboardEvent {
	return {
		key,
		code,
		shiftKey: held.shift ?? false,
		ctrlKey: held.ctrl ?? true,
		altKey: held.alt ?? false,
		metaKey: held.meta ?? false,
	} as KeyboardEvent;
}

function keysOf(id: string): Keys {
	const action = ACTIONS.find((each) => each.id === id);
	if (action === undefined) {
		throw new Error(`no action ${id}`);
	}
	return action.keys;
}

describe("what a key press is asking for", () => {
	test("Ctrl+B is bold", () => {
		expect(pressed(press("b"), keysOf("bold"))).toBe(true);
	});

	test("a shifted digit is recognised by the key on the keyboard", () => {
		// Shift+1 types "!", so nothing about the character says "one".
		expect(
			pressed(press("!", { shift: true }, "Digit1"), keysOf("h1")),
		).toBe(true);
	});

	test("a layout that types the digit itself is recognised too", () => {
		expect(pressed(press("1", { shift: true }), keysOf("h1"))).toBe(true);
	});

	test("a letter is recognised by what it types, not where it sits", () => {
		expect(
			pressed(
				press("S", { shift: true }, "KeyO"),
				keysOf("strikethrough"),
			),
		).toBe(true);
	});

	test("the shift must match exactly", () => {
		expect(pressed(press("b", { shift: true }), keysOf("bold"))).toBe(
			false,
		);
		expect(pressed(press("s"), keysOf("strikethrough"))).toBe(false);
	});

	test("holding anything else is a different shortcut", () => {
		expect(pressed(press("b", { alt: true }), keysOf("bold"))).toBe(false);
		expect(pressed(press("b", { meta: true }), keysOf("bold"))).toBe(false);
	});

	test("without the command key it is just typing", () => {
		expect(pressed(press("b", { ctrl: false }), keysOf("bold"))).toBe(
			false,
		);
	});

	test("no two actions answer to the same keys", () => {
		const all = ACTIONS.map((action) => shortcutLabel(action.keys));
		expect(new Set(all).size).toBe(all.length);
	});

	test("no action claims the underline key markdown cannot keep", () => {
		expect(ACTIONS.some((action) => pressed(press("u"), action.keys))).toBe(
			false,
		);
	});
});

describe("putting an address on words", () => {
	function selecting(markdown: string, upTo: number) {
		const editor = editorWith(markdown);
		editor.update(
			() => {
				$getRoot().getFirstChild()?.selectStart();
				const selection = $getSelection();
				if ($isRangeSelection(selection)) {
					selection.focus.offset = upTo;
				}
			},
			{ discrete: true },
		);
		return editor;
	}

	test("selected words become a link", () => {
		const editor = selecting("She unfolded the map.", 3);
		setLink(editor, "https://example.com/map");
		expect(markdownOf(editor)).toBe(
			"[She](https://example.com/map) unfolded the map.",
		);
	});

	test("an address on an existing link replaces it", () => {
		const editor = selecting("[She](https://old.example) ran.", 3);
		setLink(editor, "https://new.example");
		expect(markdownOf(editor)).toBe("[She](https://new.example) ran.");
	});

	test("no address takes the link off and leaves the words", () => {
		const editor = selecting("[She](https://example.com) ran.", 3);
		setLink(editor, null);
		expect(markdownOf(editor)).toBe("She ran.");
	});

	test("with nothing selected the address becomes its own words", () => {
		const editor = editorWith("");
		setLink(editor, "https://example.com");
		expect(markdownOf(editor)).toBe(
			"[https://example.com](https://example.com)",
		);
	});

	test("the caret inside a link reports where it points", () => {
		const editor = selecting("[She](https://example.com) ran.", 1);
		const url = editor.getEditorState().read(() => {
			const selection = $getSelection();
			return $isRangeSelection(selection) ? $linkAt(selection) : "";
		});
		expect(url).toBe("https://example.com");
	});

	test("plain words report no link", () => {
		const editor = selecting("She ran.", 1);
		const url = editor.getEditorState().read(() => {
			const selection = $getSelection();
			return $isRangeSelection(selection) ? $linkAt(selection) : "";
		});
		expect(url).toBeNull();
	});
});

describe("what the writer typed, made into an address", () => {
	test.each([
		["a bare domain gains a scheme", "example.com", "https://example.com"],
		[
			"one that has a scheme is left alone",
			"http://example.com",
			"http://example.com",
		],
		[
			"an address becomes mail",
			"her@example.com",
			"mailto:her@example.com",
		],
		["a relative path is left alone", "../notes/map.md", "../notes/map.md"],
		["an anchor is left alone", "#the-map", "#the-map"],
		["an empty field stays empty", "  ", ""],
	])("%s", (_what, typed, expected) => {
		expect(linkTarget(typed)).toBe(expected);
	});
});

describe("what may be handed to the desktop", () => {
	test.each([
		["https://example.com", true],
		["http://example.com", true],
		["mailto:her@example.com", true],
		["tel:+441234567890", true],
		["../notes/map.md", false],
		["#the-map", false],
		["file:///etc/passwd", false],
		["javascript:alert(1)", false],
	])("%s", (url, allowed) => {
		expect(openable(url)).toBe(allowed);
	});
});
