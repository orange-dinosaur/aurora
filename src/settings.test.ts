import { describe, expect, test } from "vitest";
import { bounded } from "./settings";

describe("bounded", () => {
	test("an empty field is no setting at all", () => {
		expect(bounded("", 1, 100)).toBeNull();
	});

	test("a field of nothing but punctuation is empty too", () => {
		expect(bounded("--", 1, 100)).toBeNull();
	});

	test("a number inside the range is itself", () => {
		expect(bounded("30", 1, 100)).toBe(30);
	});

	test("a grouped number keeps its digits", () => {
		expect(bounded("1,200", 1, 100000)).toBe(1200);
	});

	test("below the range comes back at the bottom of it", () => {
		expect(bounded("0", 1, 100)).toBe(1);
	});

	test("above the range comes back at the top of it", () => {
		expect(bounded("999999", 1, 100)).toBe(100);
	});

	test("a number too long to be one is still only the maximum", () => {
		expect(bounded("9".repeat(400), 1, 100)).toBe(100);
	});
});
