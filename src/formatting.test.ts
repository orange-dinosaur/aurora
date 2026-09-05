import { createHeadlessEditor } from "@lexical/headless";
import { registerList } from "@lexical/list";
import { registerRichText } from "@lexical/rich-text";
import {
	$getRoot,
	$getSelection,
	$isElementNode,
	$isRangeSelection,
} from "lexical";
import { beforeEach, describe, expect, test } from "vitest";
import {
	$blockOf,
	$formattingOf,
	$linkAt,
	ACTIONS,
	BLOCKS,
	FIND,
	INSERTS,
	KEYBOARD,
	MARKS,
	linkTarget,
	openable,
	OUTLINE,
	pressed,
	REPLACE,
	SEARCH,
	SESSION,
	SETTINGS,
	sameFormatting,
	setLink,
	shortcutLabel,
	TOOLBAR,
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

describe("putting a scene break into the text", () => {
	const [sceneBreak] = INSERTS;

	// The empty line after the break is the paragraph the caret is left in.
	test("an empty block becomes the break", () => {
		const editor = editorWith("");
		sceneBreak.run(editor);
		expect(markdownOf(editor)).toBe("---\n");
	});

	test("words are kept and the break goes below them", () => {
		const editor = editorWith("She ran.");
		sceneBreak.run(editor);
		expect(markdownOf(editor)).toBe("She ran.\n\n---\n");
	});

	// What the slash menu does: what was typed is taken out and the action
	// runs on the block it leaves empty, both in the one update.
	test("it lands once the slash that asked for it is taken away", () => {
		const editor = editorWith("");
		editor.update(
			() => {
				const block = $getRoot().getFirstChild();
				if ($isElementNode(block)) {
					const selection = $getSelection();
					if ($isRangeSelection(selection)) {
						selection.insertText("/scene");
					}
					block.getFirstChild()?.remove();
				}
				sceneBreak.run(editor);
			},
			{ discrete: true },
		);
		expect(markdownOf(editor)).toBe("---\n");
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

	test("no action claims the key that shows and hides the bar", () => {
		expect(
			ACTIONS.some(
				(action) =>
					shortcutLabel(action.keys) === shortcutLabel(TOOLBAR),
			),
		).toBe(false);
	});

	test("no action claims the key that shows and hides the outline", () => {
		expect(
			ACTIONS.some(
				(action) =>
					shortcutLabel(action.keys) === shortcutLabel(OUTLINE),
			),
		).toBe(false);
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

describe("the keyboard the settings dialog lists", () => {
	const rows = KEYBOARD.flatMap((group) => group.rows);

	test("every action the editor answers to is in it", () => {
		const listed = rows.map((row) => row.label);
		for (const action of ACTIONS) {
			expect(listed).toContain(action.label);
		}
		// The actions, and the seven bindings that do something to the app
		// rather than to the text. Nothing else is bound.
		expect(rows).toHaveLength(ACTIONS.length + 7);
	});

	test("an action is listed under the keys it is bound to", () => {
		for (const action of ACTIONS) {
			const row = rows.find((each) => each.label === action.label);
			expect(row?.keys).toEqual(action.keys);
		}
	});

	test("the bindings that are not actions are all there", () => {
		const chords = rows.map((row) => shortcutLabel(row.keys));
		for (const keys of [
			SETTINGS,
			SEARCH,
			FIND,
			REPLACE,
			OUTLINE,
			TOOLBAR,
			SESSION,
		]) {
			expect(chords).toContain(shortcutLabel(keys));
		}
	});

	// The list is also the only place every binding is side by side, which
	// makes it the only place a clash between two of them would show.
	test("no two bindings claim the same keys", () => {
		const chords = rows.map((row) => shortcutLabel(row.keys));
		expect(new Set(chords).size).toBe(chords.length);
	});
});
