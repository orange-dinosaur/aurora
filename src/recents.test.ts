import { describe, expect, test } from "vitest";
import { contents } from "./recents";

describe("contents", () => {
	test("counts the documents when there are several", () => {
		expect(contents(14)).toBe("and all 14 of its documents");
	});

	test("does not say 1 documents", () => {
		expect(contents(1)).toBe("and its one document");
	});

	test("stays vague when the manifest could not be read", () => {
		expect(contents(null)).toBe("and everything in it");
	});

	test("stays vague rather than offering all 0 of them", () => {
		expect(contents(0)).toBe("and everything in it");
	});

	test("groups a count large enough to need it", () => {
		expect(contents(1240)).toBe("and all 1,240 of its documents");
	});
});
