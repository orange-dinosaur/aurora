import { describe, expect, test } from "vitest";
import { GROUPS, filled, known, toggled } from "./groups";
import type { Book } from "./types";

const empty: Book = {
	identifier: "9f0c1a2b-0000-4000-8000-000000000000",
	title: "",
	subtitle: "",
	author: "",
	contributors: [],
	series: "",
	seriesNumber: "",
	language: "",
	blurb: "",
	cover: "",
	publisher: "",
	publicationDate: "",
	isbn: "",
	keywords: [],
	copyright: "",
	exportFormats: [],
};

describe("filled", () => {
	test("counts nothing in a book nothing has been said about", () => {
		for (const group of GROUPS) {
			expect(filled(empty, group.key)).toBe(0);
		}
	});

	test("counts a field the writer typed into", () => {
		expect(
			filled({ ...empty, title: "Ithaca", language: "en" }, "identity"),
		).toBe(2);
	});

	test("does not count whitespace as an answer", () => {
		expect(filled({ ...empty, title: "   " }, "identity")).toBe(0);
	});

	test("counts the author and the contributors together", () => {
		const book: Book = {
			...empty,
			author: "Homer",
			contributors: [
				{ name: "Chapman", role: "translator" },
				{ name: "", role: "editor" },
			],
		};

		expect(filled(book, "people")).toBe(2);
	});

	test("counts each keyword beside the blurb", () => {
		const book: Book = {
			...empty,
			blurb: "A man takes ten years to get home.",
			keywords: ["myth", "sea"],
		};

		expect(filled(book, "itself")).toBe(3);
	});

	test("counts the four things a publication has", () => {
		const book: Book = {
			...empty,
			publisher: "Nobody",
			publicationDate: "2026",
			isbn: "978-0-000-00000-0",
			copyright: "© 2026",
		};

		expect(filled(book, "publication")).toBe(4);
	});

	test("counts the cover and each format ticked", () => {
		const book: Book = {
			...empty,
			cover: "cover.png",
			exportFormats: ["markdown"],
		};

		expect(filled(book, "export")).toBe(2);
	});
});

describe("toggled", () => {
	test("folds a group that was open", () => {
		expect(toggled([], "people")).toEqual(["people"]);
	});

	test("opens a group that was folded, leaving the rest shut", () => {
		expect(toggled(["identity", "people"], "identity")).toEqual(["people"]);
	});
});

describe("known", () => {
	test("drops anything that is not a group", () => {
		expect(known(["people", "sonnets"])).toEqual(["people"]);
	});

	test("answers in the order the page draws them", () => {
		expect(known(["publication", "identity"])).toEqual([
			"identity",
			"publication",
		]);
	});
});
