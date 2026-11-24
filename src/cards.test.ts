import { describe, expect, test } from "vitest";
import {
	band,
	counted,
	described,
	eighths,
	paged,
	previewed,
	reached,
	summarised,
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
		pages: null,
		target: null,
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
		pages: null,
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

	test("a screenplay's count and target are in pages", () => {
		expect(counted(300, null, 1.375)).toBe("1 3/8 pages");
		expect(counted(12_000, 30, 12.5)).toBe("12 4/8 of 30 pages");
	});
});

describe("a length in pages", () => {
	test("is written in unreduced eighths", () => {
		expect(eighths(1.375)).toBe("1 3/8");
		expect(eighths(0.25)).toBe("2/8");
		expect(eighths(104)).toBe("104");
	});

	test("rounds to the nearest eighth, and never below one when written", () => {
		expect(eighths(1.06)).toBe("1");
		expect(eighths(0.01)).toBe("1/8");
		expect(eighths(0)).toBe("0");
	});

	test("says page up to one and pages after", () => {
		expect(paged(0.375)).toBe("3/8 page");
		expect(paged(1)).toBe("1 page");
		expect(paged(1.125)).toBe("1 1/8 pages");
		expect(paged(0)).toBe("0 pages");
	});

	test("a screenplay folder adds up its cards' pages", () => {
		const cards = [
			{ ...document("Opening", 300), pages: 1.25 },
			{ ...document("Chase", 200), pages: 0.5 },
		];
		expect(summarised(cards).pages).toBe(1.75);
		expect(summarised([document("Novel", 300)]).pages).toBeNull();
	});
});

describe("how far a row is along its target", () => {
	test("no target draws no line", () => {
		expect(reached(900, null)).toBeNull();
		expect(reached(5, 0)).toBeNull();
	});

	test("the share is the words over the target", () => {
		expect(reached(900, 1500)).toBe(0.6);
		expect(reached(0, 1500)).toBe(0);
	});

	test("writing past the target keeps the line full", () => {
		expect(reached(1800, 1500)).toBe(1);
	});
});

describe("the colour of a progress line", () => {
	test("each band starts at its own share", () => {
		expect(band(0)).toBe("red");
		expect(band(0.249)).toBe("red");
		expect(band(0.25)).toBe("orange");
		expect(band(0.5)).toBe("yellow");
		expect(band(0.999)).toBe("yellow");
		expect(band(1)).toBe("green");
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

	test("a part is an act in a screenplay and a story in a collection", () => {
		expect(described("part", 2, 40, null, "screenplay")).toBe(
			"Act · 2 items · 40 words",
		);
		expect(described("part", 2, 40, null, "short-stories")).toBe(
			"Story · 2 items · 40 words",
		);
		expect(described("chapter", 2, 40, null, "short-stories")).toBe(
			"Chapter · 2 items · 40 words",
		);
	});

	test("one item is not one items", () => {
		expect(described("part", 1, 1)).toBe("Part · 1 item · 1 word");
	});

	test("with a target the words leave for a line of their own", () => {
		expect(described("chapter", 3, 4200, 8000)).toBe("Chapter · 3 items");
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

describe("the line under a folder's name", () => {
	test("an empty folder still says so", () => {
		expect(summarised([])).toEqual({
			documents: "0 documents",
			words: 0,
			pages: null,
		});
	});

	test("the documents at this level are counted", () => {
		expect(
			summarised([
				document("Chapter 1", 900),
				document("Chapter 2", 350),
			]),
		).toEqual({ documents: "2 documents", words: 1_250, pages: null });
	});

	test("a folder card brings its words but is not a document", () => {
		expect(
			summarised([
				folder("Part One", "part", 4, 8_000),
				document("Epilogue", 120),
			]),
		).toEqual({ documents: "1 document", words: 8_120, pages: null });
	});

	test("front and back matter bring no words", () => {
		expect(
			summarised([
				folder("Front Matter", "front-matter", 2, 300),
				folder("Part One", "part", 4, 8_000),
				folder("Back Matter", "back-matter", 1, 150),
			]),
		).toEqual({ documents: "0 documents", words: 8_000, pages: null });
	});
});
