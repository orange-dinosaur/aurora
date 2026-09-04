import { describe, expect, test } from "vitest";
import { retag, tagged } from "./tags";

const CHAPTER = `---
title: Chapter One
tags:
  - Elena
  - needs-work
synopsis: She goes down to the water.
---

Elena went down to the water.
`;

describe("tagged", () => {
	test("finds a tag the document carries", () => {
		expect(tagged(CHAPTER, "Elena")).toBe(true);
		expect(tagged(CHAPTER, "needs-work")).toBe(true);
	});

	test("does not find one it does not", () => {
		expect(tagged(CHAPTER, "Wren")).toBe(false);
	});

	test("reads a tag case-sensitively", () => {
		expect(tagged(CHAPTER, "elena")).toBe(false);
	});

	test("finds nothing in a document with no front matter", () => {
		expect(tagged("Elena went down to the water.\n", "Elena")).toBe(false);
	});
});

describe("retag", () => {
	test("rewrites the tag and leaves the rest of the file alone", () => {
		const after = retag(CHAPTER, "Elena", "Elena Vance");

		expect(tagged(after, "Elena Vance")).toBe(true);
		expect(tagged(after, "Elena")).toBe(false);
		expect(tagged(after, "needs-work")).toBe(true);
		expect(after).toContain("synopsis: She goes down to the water.");
		expect(after).toContain("Elena went down to the water.\n");
	});

	test("keeps the order the tags were written in", () => {
		const after = retag(CHAPTER, "Elena", "Wren");

		expect(after.indexOf("Wren")).toBeLessThan(after.indexOf("needs-work"));
	});

	test("hands back a document without the tag character for character", () => {
		expect(retag(CHAPTER, "Wren", "Wren Ash")).toBe(CHAPTER);
	});

	test("does not rewrite a tag that differs only in case", () => {
		expect(retag(CHAPTER, "elena", "Wren")).toBe(CHAPTER);
	});

	test("does not leave a document wearing the same tag twice", () => {
		const both = retag(CHAPTER, "needs-work", "Elena");

		expect(tagged(both, "Elena")).toBe(true);
		expect(tagged(both, "needs-work")).toBe(false);
		expect(both.match(/Elena$/gm)).toHaveLength(1);
	});

	test("leaves a document with no front matter alone", () => {
		const plain = "Elena went down to the water.\n";

		expect(retag(plain, "Elena", "Wren")).toBe(plain);
	});
});
