import { createHeadlessEditor } from "@lexical/headless";
import { $createHeadingNode, registerRichText } from "@lexical/rich-text";
import { $getRoot, $getSelection, $isRangeSelection } from "lexical";
import { describe, expect, test } from "vitest";
import { BLOCKS, INSERTS } from "./formatting";
import { $fromMarkdown, EDITOR_NODES } from "./markdown";
import { $slashAt, matching, slashQuery } from "./slash";

describe("recognising a slash that is asking for the menu", () => {
	test("a slash on its own opens it with nothing typed", () => {
		expect(slashQuery("/", "/")).toBe("");
	});

	test("what follows the slash is the search", () => {
		expect(slashQuery("/head", "/head")).toBe("head");
	});

	test("a slash part-way through a line is just a slash", () => {
		expect(slashQuery("and /head", "and /head")).toBeNull();
	});

	test("words already after the caret close it", () => {
		expect(slashQuery("/head", "/heading north")).toBeNull();
	});

	test("nothing typed yet is not a search", () => {
		expect(slashQuery("", "")).toBeNull();
	});

	test("a long enough line stops being one", () => {
		const long = `/${"a".repeat(25)}`;
		expect(slashQuery(long, long)).toBeNull();
	});
});

// The same reading, taken from a real document rather than from two strings.
function caretAfter(write: () => void): string | null {
	const editor = createHeadlessEditor({
		namespace: "aurora",
		nodes: EDITOR_NODES,
		onError: (error) => {
			throw error;
		},
	});
	registerRichText(editor);
	editor.update(write, { discrete: true });
	return editor.getEditorState().read($slashAt);
}

function type(text: string): void {
	const selection = $getSelection();
	if ($isRangeSelection(selection)) {
		selection.insertText(text);
	}
}

describe("reading the caret", () => {
	test("a slash typed into an empty document asks for the menu", () => {
		expect(
			caretAfter(() => {
				$fromMarkdown("");
				$getRoot().selectEnd();
				type("/qu");
			}),
		).toBe("qu");
	});

	test("a slash typed in front of words does not", () => {
		expect(
			caretAfter(() => {
				$fromMarkdown("She ran.");
				$getRoot().selectStart();
				type("/qu");
			}),
		).toBeNull();
	});

	test("only the block the caret is in is read", () => {
		expect(
			caretAfter(() => {
				$fromMarkdown("She ran.");
				$getRoot().selectEnd();
				const selection = $getSelection();
				if ($isRangeSelection(selection)) {
					selection.insertParagraph();
				}
				type("/qu");
			}),
		).toBe("qu");
	});

	test("an empty heading asks as readily as an empty paragraph", () => {
		expect(
			caretAfter(() => {
				const heading = $createHeadingNode("h2");
				$getRoot().clear().append(heading);
				heading.select();
				type("/qu");
			}),
		).toBe("qu");
	});

	test("a caret with no slash in front of it asks for nothing", () => {
		expect(
			caretAfter(() => {
				$fromMarkdown("");
				$getRoot().selectEnd();
				type("She ran.");
			}),
		).toBeNull();
	});
});

describe("choosing what the menu lists", () => {
	test("nothing typed lists everything, the blocks first", () => {
		expect(matching("")).toEqual([...BLOCKS, ...INSERTS]);
	});

	test("a name is matched from its start", () => {
		expect(matching("head").map((action) => action.id)).toEqual([
			"h1",
			"h2",
			"h3",
		]);
	});

	test("any word of a name can be the one that matches", () => {
		expect(matching("list").map((action) => action.id)).toEqual([
			"bullet",
			"number",
		]);
	});

	test("the number of a heading finds it", () => {
		expect(matching("2").map((action) => action.id)).toEqual(["h2"]);
	});

	test("case is not part of the question", () => {
		expect(matching("QUO").map((action) => action.id)).toEqual(["quote"]);
	});

	test("what is inserted is offered alongside what is turned into", () => {
		expect(matching("scene").map((action) => action.id)).toEqual(["rule"]);
		expect(matching("break").map((action) => action.id)).toEqual(["rule"]);
	});

	test("a name nothing answers to lists nothing", () => {
		expect(matching("footnote")).toEqual([]);
	});
});
