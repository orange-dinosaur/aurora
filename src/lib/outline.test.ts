import { describe, expect, test } from "vitest";
import { containing, outlined, type Block } from "./outline";

/** A document written as its top-level blocks, headings marked by their level. */
function document(...blocks: [string, number | null, string][]): Block[] {
	return blocks.map(([key, level, text]) => ({ key, level, text }));
}

function heading(key: string, level: number, text: string) {
	return [key, level, text] as [string, number | null, string];
}

function paragraph(key: string) {
	return [key, null, "…"] as [string, number | null, string];
}

describe("listing the headings", () => {
	test("a document with no headings has no outline", () => {
		expect(outlined(document(paragraph("p1"), paragraph("p2")))).toEqual(
			[],
		);
	});

	test("headings of one level are all drawn flush", () => {
		const list = outlined(
			document(
				heading("h1", 1, "One"),
				paragraph("p1"),
				heading("h2", 1, "Two"),
			),
		);

		expect(list.map((entry) => entry.text)).toEqual(["One", "Two"]);
		expect(list.map((entry) => entry.depth)).toEqual([0, 0]);
	});

	test("a heading under another is drawn a step in", () => {
		const list = outlined(
			document(
				heading("a", 1, "Chapter"),
				heading("b", 2, "Scene"),
				heading("c", 3, "Beat"),
				heading("d", 2, "Scene"),
				heading("e", 1, "Chapter"),
			),
		);

		expect(list.map((entry) => entry.depth)).toEqual([0, 1, 2, 1, 0]);
	});

	test("a document that never uses Heading 1 still starts flush", () => {
		const list = outlined(
			document(heading("a", 2, "Scene"), heading("b", 3, "Beat")),
		);

		expect(list.map((entry) => entry.depth)).toEqual([0, 1]);
	});

	// Writers skip levels. Indenting by the level itself would leave a gap
	// where Heading 2 would have been.
	test("a skipped level is one step, not two", () => {
		const list = outlined(
			document(heading("a", 1, "Chapter"), heading("b", 3, "Beat")),
		);

		expect(list.map((entry) => entry.depth)).toEqual([0, 1]);
	});

	test("the key and the level are carried through", () => {
		expect(outlined(document(heading("a", 2, "Scene")))).toEqual([
			{ key: "a", level: 2, text: "Scene", depth: 0 },
		]);
	});
});

describe("finding where the writer is", () => {
	const chapter = document(
		paragraph("p0"),
		heading("a", 1, "Chapter"),
		paragraph("p1"),
		heading("b", 2, "Scene"),
		paragraph("p2"),
	);

	test("a paragraph belongs to the heading above it", () => {
		expect(containing(chapter, "p1")).toBe("a");
		expect(containing(chapter, "p2")).toBe("b");
	});

	test("a heading is its own section", () => {
		expect(containing(chapter, "b")).toBe("b");
	});

	test("nothing is marked above the first heading", () => {
		expect(containing(chapter, "p0")).toBe(null);
	});

	test("a caret that is nowhere marks nothing", () => {
		expect(containing(chapter, null)).toBe(null);
		expect(containing(chapter, "gone")).toBe(null);
	});
});
