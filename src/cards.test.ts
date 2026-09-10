import { describe, expect, test } from "vitest";
import {
	abbreviated,
	counted,
	described,
	previewed,
	summarised,
	trashed,
} from "./cards";
import type { FolderKind, OverviewCard } from "./types";

function folder(
	name: string,
	kind: FolderKind | null,
	held: number,
	words = 0,
) {
	return {
		node: "folder",
		id: `id-${name}`,
		name,
		kind,
		children: held,
		words,
	} satisfies OverviewCard;
}

function document(title: string, words: number) {
	return {
		node: "document",
		id: `id-${title}`,
		path: `Manuscript/${title}.md`,
		trail: ["Manuscript"],
		title,
		target: null,
		words,
		excerpt: "",
		front: "",
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

describe("the line under a document card's title", () => {
	const OPENING = "Elena went down to the water.";

	test("is the synopsis when the writer has written one", () => {
		const front = "---\nsynopsis: She learns the boat is gone.\n---";

		expect(previewed(front, OPENING)).toBe("She learns the boat is gone.");
	});

	test("falls back to the opening when they have not", () => {
		expect(previewed("---\ntags:\n  - draft\n---", OPENING)).toBe(OPENING);
		expect(previewed("", OPENING)).toBe(OPENING);
	});

	test("falls back when the synopsis has been emptied to nothing", () => {
		expect(previewed('---\nsynopsis: "   "\n---', OPENING)).toBe(OPENING);
	});

	test("collapses a synopsis written over several lines", () => {
		const front =
			'---\nsynopsis: "She waits.\\n\\nThen she does not."\n---';

		expect(previewed(front, OPENING)).toBe("She waits. Then she does not.");
	});
});

describe("a folder card's line", () => {
	test("a folder with no kind says only that it is a folder", () => {
		expect(described(null, 3, 1200)).toBe("Folder · 3 items · 1,200 words");
	});

	test("a part and a chapter say which they are", () => {
		expect(described("part", 2, 40)).toBe("Part · 2 items · 40 words");
		expect(described("chapter", 0, 0)).toBe("Chapter · 0 items · 0 words");
	});

	test("one item is not one items", () => {
		expect(described("part", 1, 1)).toBe("Part · 1 item · 1 word");
	});

	test("front and back matter are named, not taken for chapters", () => {
		expect(described("front-matter", 2, 300)).toBe(
			"Front matter · 2 items · 300 words",
		);
		expect(described("back-matter", 1, 150)).toBe(
			"Back matter · 1 item · 150 words",
		);
	});
});

describe("a folder row's count", () => {
	test("a short folder says exactly what it holds", () => {
		expect(abbreviated(0)).toBe("0");
		expect(abbreviated(999)).toBe("999");
	});

	test("a longer one is cut down to thousands", () => {
		expect(abbreviated(1200)).toBe("1.2k");
		expect(abbreviated(48_300)).toBe("48k");
	});

	test("what is cut is never rounded up", () => {
		expect(abbreviated(1999)).toBe("1.9k");
		expect(abbreviated(9999)).toBe("9.9k");
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

	test("a folder card brings its words but is not a document", () => {
		expect(
			summarised([
				folder("Part One", "part", 4, 8_000),
				document("Epilogue", 120),
			]),
		).toBe("1 document · 8,120 words");
	});

	test("front and back matter bring no words", () => {
		expect(
			summarised([
				folder("Front Matter", "front-matter", 2, 300),
				folder("Part One", "part", 4, 8_000),
				folder("Back Matter", "back-matter", 1, 150),
			]),
		).toBe("0 documents · 8,000 words");
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
