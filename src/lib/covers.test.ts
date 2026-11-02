import { describe, expect, test } from "vitest";
import { coverFor } from "./covers";

describe("coverFor", () => {
	test("the same title always gets the same cover", () => {
		expect(coverFor("The Salt Almanac")).toBe(coverFor("The Salt Almanac"));
	});

	test("different titles get different covers", () => {
		expect(coverFor("The Salt Almanac")).not.toBe(coverFor("Cold Water"));
	});

	test("a long title stays a valid background", () => {
		expect(coverFor("a".repeat(5000))).not.toMatch(/undefined|NaN| -\d/);
	});

	test("an empty title still draws a cover", () => {
		expect(coverFor("")).toContain("linear-gradient(0deg");
	});
});
