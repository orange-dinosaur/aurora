import { describe, expect, test } from "vitest";
import { creatable } from "./kinds";

/** What the + would offer, written the way the menu reads. */
function offered(kind: "part" | "chapter" | null, section: string) {
	return creatable(kind, section).map((making) => making.label);
}

describe("what a + offers in the Manuscript", () => {
	test("the Manuscript itself takes a part, a chapter or a document", () => {
		expect(offered(null, "Manuscript")).toEqual([
			"New document",
			"New part",
			"New chapter",
		]);
	});

	test("a part takes chapters, not more parts", () => {
		expect(offered("part", "Manuscript")).toEqual([
			"New document",
			"New chapter",
		]);
	});

	test("a chapter takes documents and nothing else", () => {
		expect(offered("chapter", "Manuscript")).toEqual(["New document"]);
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
