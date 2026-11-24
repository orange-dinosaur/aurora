import { createHeadlessEditor } from "@lexical/headless";
import { $createHeadingNode, registerRichText } from "@lexical/rich-text";
import { $getRoot, $getSelection, $isRangeSelection } from "lexical";
import { describe, expect, test } from "vitest";
import { BLOCKS, INSERTS } from "./formatting";
import { $fromMarkdown, EDITOR_NODES } from "./markdown";
import { cueFor, headingAt } from "./fountain";
import { cueEntries, missingFrom, scriptEntries } from "./screenplay";
import { $slashAt, matching, NOVEL, slashQuery } from "./slash";

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
	const ids = (query: string) =>
		matching(query, NOVEL).map((action) => action.id);

	test("nothing typed lists everything, the blocks first", () => {
		expect(matching("", NOVEL)).toEqual([...BLOCKS, ...INSERTS]);
	});

	test("a name is matched from its start", () => {
		expect(ids("head")).toEqual(["h1", "h2", "h3"]);
	});

	test("any word of a name can be the one that matches", () => {
		expect(ids("list")).toEqual(["bullet", "number"]);
	});

	test("the number of a heading finds it", () => {
		expect(ids("2")).toEqual(["h2"]);
	});

	test("case is not part of the question", () => {
		expect(ids("QUO")).toEqual(["quote"]);
	});

	test("what is inserted is offered alongside what is turned into", () => {
		expect(ids("scene")).toEqual(["rule"]);
		expect(ids("break")).toEqual(["rule"]);
	});

	test("a name nothing answers to lists nothing", () => {
		expect(matching("footnote", NOVEL)).toEqual([]);
	});
});

describe("what a slash offers in a script", () => {
	const filed = (name: string, folder = "") => ({ name, folder });
	const offered = scriptEntries(
		[filed("Elena Marsh", "Family"), filed("Marcus")],
		[filed("Kitchen"), filed("Paris", "Abroad/France")],
	);
	const hints = (query: string) =>
		matching(query, offered).map((entry) => entry.hint);
	const ids = (query: string) =>
		matching(query, offered).map((action) => action.id);

	test("nothing typed lists only the blocks, however large the cast", () => {
		expect(ids("")).toEqual([
			"heading",
			"action",
			"character",
			"dialogue",
			"parenthetical",
			"transition",
		]);
	});

	test("a name shows its folder, or its section's word when it has none", () => {
		expect(hints("mar")).toEqual(["Family", "Character"]);
		expect(hints("k")).toEqual(["Location"]);
		expect(hints("paris")).toEqual(["Abroad/France"]);
	});

	test("a single typed letter brings the names in", () => {
		expect(ids("m")).toEqual(["cue:Elena Marsh", "cue:Marcus"]);
	});

	test("a character is found by any word of their name", () => {
		expect(ids("el")).toEqual(["cue:Elena Marsh"]);
		expect(ids("mar")).toEqual(["cue:Elena Marsh", "cue:Marcus"]);
	});

	test("a block and a name can both answer", () => {
		expect(ids("pa")).toEqual(["parenthetical", "place:Paris"]);
	});

	test("a cue line offers the characters whose cue starts with it", () => {
		const cues = (typed: string) =>
			cueEntries(typed, [
				filed("Elena Marsh"),
				filed("Elias"),
				filed("Marcus"),
			]).map((entry) => entry.label);
		expect(cues("EL")).toEqual(["Elena Marsh", "Elias"]);
		expect(cues("eli")).toEqual(["Elias"]);
		expect(cues("MARSH")).toEqual([]);
		expect(cues("ELIAS")).toEqual([]);
		expect(cues(" ")).toEqual([]);
	});

	test("a cue or heading naming no page says what to add, and where", () => {
		const names = {
			characters: [filed("Elena Marsh")],
			locations: [filed("Kitchen")],
		};
		expect(
			missingFrom("character", "ELENA MARSH (V.O.)", names),
		).toBeNull();
		expect(missingFrom("character", "MARCUS", names)).toEqual({
			section: "Characters",
			name: "Marcus",
		});
		expect(missingFrom("heading", "INT. KITCHEN - DAY", names)).toBeNull();
		expect(missingFrom("heading", "EXT. OLD MILL - DAY", names)).toEqual({
			section: "Locations",
			name: "Old Mill",
		});
		expect(missingFrom("action", "MARCUS runs.", names)).toBeNull();
		expect(missingFrom("heading", "INT. ", names)).toBeNull();
	});

	test("a cue is the name in capitals and a heading starts at the place", () => {
		expect(cueFor("Elena Marsh")).toBe("ELENA MARSH");
		expect(headingAt("Kitchen")).toBe("INT. KITCHEN - ");
	});
});
