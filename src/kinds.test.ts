import { describe, expect, test } from "vitest";
import { creatable, mayHold } from "./kinds";

/** What the + would offer, written the way the menu reads. */
function offered(kind: "part" | "chapter" | null, section: string) {
	return creatable(kind, section).map((making) => making.label);
}

describe("what a + offers in the Manuscript", () => {
	test("the Manuscript itself takes a part, a chapter or a scene", () => {
		expect(offered(null, "Manuscript")).toEqual([
			"New scene",
			"New part",
			"New chapter",
		]);
	});

	test("a part takes chapters, not more parts", () => {
		expect(offered("part", "Manuscript")).toEqual([
			"New scene",
			"New chapter",
		]);
	});

	test("a chapter takes scenes and nothing else", () => {
		expect(offered("chapter", "Manuscript")).toEqual(["New scene"]);
	});

	test("a document is named for what it would be where it sits", () => {
		const [inTheManuscript] = creatable(null, "Manuscript");
		const [inAChapter] = creatable("chapter", "Manuscript");
		const [inNotes] = creatable(null, "Notes");

		expect(inTheManuscript.noun).toBe("scene");
		expect(inAChapter.noun).toBe("scene");
		expect(inAChapter.placeholder).toBe("Scene 2");
		expect(inNotes.noun).toBe("document");
		expect(inNotes.placeholder).toBe("Ideas");
	});
});

describe("what a + offers everywhere else", () => {
	test("a section outside the Manuscript offers a plain folder", () => {
		expect(offered(null, "Notes")).toEqual(["New document", "New folder"]);
		// A folder there is never a part or a chapter.
		expect(creatable(null, "Notes").map((making) => making.kind)).toEqual([
			null,
			null,
		]);
	});

	test("a folder there offers the same, however deep it sits", () => {
		expect(offered(null, "Characters")).toEqual([
			"New document",
			"New folder",
		]);
	});
});

describe("what a folder may hold", () => {
	// The whole matrix, which is the same one `tree::may_hold` is held to in
	// Rust. Both a + and a move ask this.
	test.each([
		["the Manuscript takes a part", true, null, "part", true],
		["the Manuscript takes a chapter", true, null, "chapter", true],
		["the Manuscript takes no plain folder", true, null, null, false],
		["a part takes a chapter", true, "part", "chapter", true],
		["a part takes no part", true, "part", "part", false],
		["a part takes no plain folder", true, "part", null, false],
		["a chapter holds documents only", true, "chapter", "chapter", false],
		["a chapter takes no part", true, "chapter", "part", false],
		["a chapter takes no plain folder", true, "chapter", null, false],
		["the Manuscript takes front matter", true, null, "front-matter", true],
		["the Manuscript takes back matter", true, null, "back-matter", true],
		["a part takes no front matter", true, "part", "front-matter", false],
		["matter holds documents only", true, "front-matter", null, false],
		["matter takes no chapter", true, "front-matter", "chapter", false],
		["matter takes no matter", true, "back-matter", "front-matter", false],
		["a section elsewhere takes a folder", false, null, null, true],
		["a folder elsewhere takes a folder", false, null, null, true],
		["nowhere else takes a part", false, null, "part", false],
		["nowhere else takes a chapter", false, null, "chapter", false],
		["nowhere else takes matter", false, null, "front-matter", false],
	] as const)("%s", (_what, inManuscript, parent, kind, allowed) => {
		expect(mayHold(inManuscript, parent, kind)).toBe(allowed);
	});
});
