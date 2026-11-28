import { describe, expect, test } from "vitest";
import type { RecentSummary } from "./bindings/RecentSummary";
import { bookLength, bookTitle, formatCounts, shelved } from "./shelf";

function book(fields: Partial<RecentSummary>): RecentSummary {
	return {
		name: "Untitled",
		root: "/books/untitled",
		lastOpened: null,
		elsewhere: false,
		words: 1000,
		pages: null,
		documents: 3,
		title: null,
		author: null,
		cover: null,
		format: "novel",
		leftOff: null,
		...fields,
	};
}

const almanac = book({
	name: "almanac",
	title: "The Salt Almanac",
	author: "Ines Marlowe",
	words: 41280,
});
const ferry = book({
	name: "ferry",
	title: "The Night Ferry",
	author: "Pell Avery",
	format: "screenplay",
	words: 20000,
	pages: 104,
});
const hours = book({
	name: "Small Hours",
	format: "short-stories",
	words: 18450,
});
const lost = book({
	root: "/media/usb/old-draft",
	name: "Old draft",
	words: null,
	format: null,
});
const all = [almanac, ferry, hours, lost];

describe("the shelf's books", () => {
	test("all, in the order they came, is every book", () => {
		expect(shelved(all, "all", "", "opened")).toEqual(all);
	});

	test("a format keeps its own books, and never an unread one", () => {
		expect(shelved(all, "screenplay", "", "opened")).toEqual([ferry]);
		expect(shelved(all, "novel", "", "opened")).toEqual([almanac]);
	});

	test("search matches title or author, any case", () => {
		expect(shelved(all, "all", "  salt ", "opened")).toEqual([almanac]);
		expect(shelved(all, "all", "AVERY", "opened")).toEqual([ferry]);
		expect(shelved(all, "all", "hours", "opened")).toEqual([hours]);
		expect(shelved(all, "all", "old-draft", "opened")).toEqual([lost]);
		expect(shelved(all, "novel", "ferry", "opened")).toEqual([]);
	});

	test("title sorts by the name each book goes by", () => {
		expect(shelved(all, "all", "", "title").map(bookTitle)).toEqual([
			"old-draft",
			"Small Hours",
			"The Night Ferry",
			"The Salt Almanac",
		]);
	});

	test("length counts a screenplay page as 250 words, longest first", () => {
		expect(bookLength(ferry)).toBe(26000);
		expect(shelved(all, "all", "", "length")).toEqual([
			almanac,
			ferry,
			hours,
			lost,
		]);
	});

	test("sorting leaves the list it was given alone", () => {
		const given = [...all];
		shelved(given, "all", "", "title");
		expect(given).toEqual(all);
	});

	test("each format counts its books, an unread one only under all", () => {
		expect(formatCounts(all)).toEqual({
			all: 4,
			novel: 1,
			screenplay: 1,
			"short-stories": 1,
			"stage-play": 0,
		});
	});
});
