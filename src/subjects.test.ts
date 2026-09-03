import { describe, expect, test } from "vitest";
import { isSubject } from "./subjects";

describe("what counts as a subject", () => {
	test("a document in Characters or Locations is one", () => {
		expect(isSubject(["Characters"])).toBe(true);
		expect(isSubject(["Locations"])).toBe(true);
	});

	test("filing it deeper does not change that", () => {
		expect(isSubject(["Characters", "House Marsh"])).toBe(true);
	});

	test("the story and the writer's own notes are not", () => {
		expect(isSubject(["Manuscript"])).toBe(false);
		expect(isSubject(["Manuscript", "Part One", "Chapter 3"])).toBe(false);
		expect(isSubject(["Notes"])).toBe(false);
		expect(isSubject(["Outline"])).toBe(false);
	});

	test("a folder of the same name deeper in is not the section", () => {
		expect(isSubject(["Notes", "Characters"])).toBe(false);
	});

	test("a document that sits nowhere is not one", () => {
		expect(isSubject([])).toBe(false);
	});
});
