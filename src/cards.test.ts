import { describe, expect, test } from "vitest";
import { counted, described, summarised, trashed } from "./cards";
import type { OverviewCard } from "./types";

function folder(name: string, kind: "part" | "chapter" | null, held: number) {
	return {
		node: "folder",
		id: `id-${name}`,
		name,
		kind,
		children: held,
	} satisfies OverviewCard;
}

function document(title: string, words: number) {
	return {
		node: "document",
		id: `id-${title}`,
		path: `Manuscript/${title}.md`,
		folder: "Manuscript",
		title,
		target: null,
		words,
		excerpt: "",
		modified: null,
	} satisfies OverviewCard;
}

describe("a document card's count", () => {
	test("one word is not one words", () => {
		expect(counted(1, null)).toBe("1 word");
	});

	test("a target turns the count into a report against it", () => {
		expect(counted(1200, 2000)).toBe("1,200 of 2,000 words");
	});
});

describe("a folder card's line", () => {
	test("a folder with no kind says only that it is a folder", () => {
		expect(described(null, 3)).toBe("Folder · 3 items");
	});

	test("a part and a chapter say which they are", () => {
		expect(described("part", 2)).toBe("Part · 2 items");
		expect(described("chapter", 0)).toBe("Chapter · 0 items");
	});

	test("one item is not one items", () => {
		expect(described("part", 1)).toBe("Part · 1 item");
	});
});

describe("the line under a folder's name", () => {
	test("an empty folder still says so", () => {
		expect(summarised([])).toBe("0 documents · 0 words");
	});

	test("the documents at this level are counted", () => {
		expect(
			summarised([
				document("Chapter 1", 900),
				document("Chapter 2", 350),
			]),
		).toBe("2 documents · 1,250 words");
	});

	test("a folder card adds nothing, since its words have not been read", () => {
		expect(
			summarised([
				folder("Part One", "part", 4),
				document("Epilogue", 120),
			]),
		).toBe("1 document · 120 words");
	});
});

describe("what a folder in the trash took with it", () => {
	test("an empty folder still says it is one", () => {
		expect(trashed(0)).toBe("an empty folder");
	});

	test("one document is not one documents", () => {
		expect(trashed(1)).toBe("a folder of 1 document");
	});

	test("a chapter of fourteen scenes counts every one of them", () => {
		expect(trashed(14)).toBe("a folder of 14 documents");
	});
});
