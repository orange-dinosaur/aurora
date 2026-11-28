import { describe, expect, test } from "vitest";
import {
	creatable,
	documentPlaceholder,
	folderKinds,
	folderPlaceholder,
	mayHold,
	suggested,
} from "./kinds";
import type { FolderKind, Format } from "./types";

describe("the name a new thing's field opens with", () => {
	test("the noun, capitalised, numbered after what the folder holds", () => {
		expect(suggested("scene", ["A", "B", "C"])).toBe("Scene 4");
		expect(suggested("chapter", [])).toBe("Chapter 1");
	});

	test("past a name the folder already has, whatever its case", () => {
		expect(
			suggested("chapter", [
				"Chapter 4",
				"Chapter 5 The Storm",
				"chapter 6",
			]),
		).toBe("Chapter 7");
		expect(suggested("scene", ["Scene 1", "Scene 10"])).toBe("Scene 3");
	});
});

/** What the + would offer, written the way the menu reads. */
function offered(
	kind: FolderKind | null,
	section: string,
	held: (FolderKind | null)[] = [],
) {
	return creatable(kind, section, held).map((making) => making.label);
}

describe("what a + offers in the Manuscript", () => {
	test("the Manuscript itself takes a part, a chapter, a scene or matter", () => {
		expect(offered(null, "Manuscript")).toEqual([
			"New scene",
			"New part",
			"New chapter",
			"New front matter",
			"New back matter",
		]);
	});

	test("matter it already holds is not offered again", () => {
		expect(offered(null, "Manuscript", ["front-matter"])).toEqual([
			"New scene",
			"New part",
			"New chapter",
			"New back matter",
		]);
		expect(
			offered(null, "Manuscript", ["front-matter", "back-matter"]),
		).toEqual(["New scene", "New part", "New chapter"]);
	});

	test("a part or a chapter it holds changes nothing", () => {
		expect(offered(null, "Manuscript", ["part", "chapter", null])).toEqual([
			"New scene",
			"New part",
			"New chapter",
			"New front matter",
			"New back matter",
		]);
	});

	test("matter is made under the name Aurora gives it", () => {
		const made = creatable(null, "Manuscript").filter(
			(making) => making.name,
		);

		expect(made.map((making) => [making.kind, making.name])).toEqual([
			["front-matter", "Front Matter"],
			["back-matter", "Back Matter"],
		]);
	});

	test("everything else asks for a name", () => {
		const asked = creatable(null, "Notes").concat(
			creatable("part", "Manuscript"),
		);

		expect(asked.every((making) => making.name === undefined)).toBe(true);
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
	test("matter is offered nowhere but the Manuscript", () => {
		expect(offered(null, "Notes")).toEqual(["New document", "New folder"]);
		expect(offered("part", "Manuscript")).not.toContain("New front matter");
		expect(offered("front-matter", "Manuscript")).toEqual(["New scene"]);
	});

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

describe("what a folder already holds", () => {
	test("only its folders count, and a plain one counts as no kind", () => {
		expect(
			folderKinds([
				{ node: "folder", kind: "part" },
				{ node: "document" },
				{ node: "folder", kind: "front-matter" },
				{ node: "folder", kind: null },
			]),
		).toEqual(["part", "front-matter", null]);
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
		expect(mayHold("novel", inManuscript, parent, kind)).toBe(allowed);
	});

	const KINDS = [
		null,
		"part",
		"chapter",
		"front-matter",
		"back-matter",
	] as const;

	test("a screenplay's Manuscript takes only an act", () => {
		for (const kind of KINDS) {
			expect(mayHold("screenplay", true, null, kind)).toBe(
				kind === "part",
			);
		}
	});

	test("a screenplay's act takes no folder at all", () => {
		for (const kind of KINDS) {
			expect(mayHold("screenplay", true, "part", kind)).toBe(false);
		}
	});

	test("a collection answers as a novel does", () => {
		for (const inManuscript of [true, false]) {
			for (const parent of KINDS) {
				for (const kind of KINDS) {
					expect(
						mayHold("short-stories", inManuscript, parent, kind),
					).toBe(mayHold("novel", inManuscript, parent, kind));
				}
			}
		}
	});
});

describe("what a + offers in each format", () => {
	function labels(
		format: Format,
		kind: FolderKind | null,
		section = "Manuscript",
	) {
		return creatable(kind, section, [], format).map(
			(making) => making.label,
		);
	}

	test("a screenplay's Manuscript offers a scene or an act", () => {
		expect(labels("screenplay", null)).toEqual(["New scene", "New act"]);
	});

	test("inside an act a screenplay offers only a scene", () => {
		expect(labels("screenplay", "part")).toEqual(["New scene"]);
	});

	test("a collection calls a part a story", () => {
		expect(labels("short-stories", null)).toEqual([
			"New scene",
			"New story",
			"New chapter",
			"New front matter",
			"New back matter",
		]);
		expect(labels("short-stories", "part")).toEqual([
			"New scene",
			"New chapter",
		]);
	});

	test("a screenplay's Notes still takes a folder", () => {
		expect(labels("screenplay", null, "Notes")).toEqual([
			"New document",
			"New folder",
		]);
	});

	test("the name field suggests the next act or story", () => {
		const act = creatable(null, "Manuscript", [], "screenplay")[1];
		expect([act.noun, act.placeholder]).toEqual(["act", "Act Two"]);
		const story = creatable(null, "Manuscript", [], "short-stories")[1];
		expect([story.noun, story.placeholder]).toEqual(["story", "Story 2"]);
		expect(folderPlaceholder("part", "screenplay")).toBe("Act Two");
		expect(folderPlaceholder("part", "short-stories")).toBe("Story 2");
		expect(folderPlaceholder("part")).toBe("Part One");
	});
});

describe("what a rename field suggests for a document", () => {
	test("a scene in the Manuscript, a document elsewhere", () => {
		expect(documentPlaceholder("Manuscript")).toBe("Scene 2");
		expect(documentPlaceholder("Notes")).toBe("Ideas");
		expect(documentPlaceholder(undefined)).toBe("Ideas");
	});
});
