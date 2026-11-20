import { describe, expect, test } from "vitest";
import { reading } from "./Target";

describe("the words half of the status line", () => {
	test("no target says only the count", () => {
		expect(reading(1, null)).toBe("1 word");
		expect(reading(1200, null)).toBe(`${(1200).toLocaleString()} words`);
	});

	test("a target adds what is wanted and the share", () => {
		expect(reading(5, 10)).toBe("5 of 10 words · 50%");
	});

	test("a target of zero is no target", () => {
		expect(reading(5, 0)).toBe("5 words");
		expect(reading(5, 0)).not.toContain("Infinity");
	});
});
