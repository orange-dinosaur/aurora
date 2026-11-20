import { describe, expect, test } from "vitest";
import { characters, charactersWithoutSpaces, prose, words } from "./words";

/** A character named by code point, so the test file holds nothing invisible. */
function ch(code: number): string {
	return String.fromCharCode(code);
}

describe("counting words", () => {
	test("an empty document has none", () => {
		expect(words("")).toBe(0);
		expect(words("   \n\n  ")).toBe(0);
	});

	test("runs of whitespace are one break, wherever they fall", () => {
		expect(words("  one   two\n\nthree \t four  ")).toBe(4);
	});

	test("markdown punctuation travels with its word", () => {
		expect(words("# A title")).toBe(3);
		expect(words("- **bold** item")).toBe(3);
	});

	// The two counts are only trustworthy if they agree here. Rust's
	// `char::is_whitespace` follows the Unicode property, which JavaScript's
	// `\s` matches in neither direction.
	test("it breaks on next line but not on the byte-order mark", () => {
		expect(words(`one${ch(0x85)}two`)).toBe(2);
		expect(words(`one${ch(0xfeff)}two`)).toBe(1);
	});

	test("it breaks on the other Unicode spaces", () => {
		expect(words(`one${ch(0xa0)}two${ch(0x3000)}three`)).toBe(3);
	});
});

describe("counting characters", () => {
	test("an empty document has none", () => {
		expect(characters("")).toBe(0);
	});

	test("spaces and newlines count", () => {
		expect(characters("a b\nc")).toBe(5);
	});

	// "hi 👋" is five UTF-16 units but four characters, and the writer would
	// only ever agree with the second number.
	test("a character outside the basic plane counts once", () => {
		const wave = String.fromCodePoint(0x1f44b);
		expect(characters(`hi ${wave}`)).toBe(4);
		expect(`hi ${wave}`.length).toBe(5);
	});
});

describe("counting characters without spaces", () => {
	test("whitespace of every kind drops out", () => {
		expect(charactersWithoutSpaces("a b\n\tc")).toBe(3);
		expect(charactersWithoutSpaces(`a${ch(0xa0)}b`)).toBe(2);
	});

	test("punctuation is still a character", () => {
		expect(charactersWithoutSpaces("it's — here!")).toBe(10);
	});

	test("a document of nothing but spaces has none", () => {
		expect(charactersWithoutSpaces("   \n\n  ")).toBe(0);
	});
});

describe("what counts as the document", () => {
	const page = [
		"---",
		'remarks: "a note to myself"',
		"---",
		"",
		"# Chapter One",
		"",
		"Elena andava a scuola",
	].join("\n");

	test("naming a field is not writing a word", () => {
		expect(words(page)).toBe(14);
		expect(words(prose(page))).toBe(6);
	});

	test("a document with no front matter is all of it", () => {
		expect(prose("Elena andava a scuola")).toBe("Elena andava a scuola");
	});
});

// The same table is in `document.rs`. Both sides count markdown the same way
// or the status line and the section cards disagree.
describe("markdown markers are not words", () => {
	const table: [string, number][] = [
		["# Title", 1],
		["###### Deep heading", 2],
		["####### seven hashes", 3],
		["# Title\n\n* * *\n\nHello world", 3],
		["---", 0],
		["***", 0],
		["_ _ _", 0],
		["> quoted line", 2],
		["> > nested quote", 2],
		["- one\n- two", 2],
		["* one\n+ two", 2],
		["1. one\n2. two", 2],
		["**bold** and _italic_", 3],
		["~~gone~~ now", 2],
		["`code` here", 2],
		["a \\*b c", 3],
		["Elena\\.", 1],
		["2024. A year", 2],
		["2024.5 is a number", 4],
	];

	test.each(table)("%j counts %i", (markdown, count) => {
		expect(words(prose(markdown))).toBe(count);
	});
});
