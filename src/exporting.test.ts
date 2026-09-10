import { describe, expect, test } from "vitest";
import { flipped, sized, wrote } from "./exporting";

describe("flipped", () => {
	test("ticks a format that was not ticked", () => {
		expect(flipped([], "markdown")).toEqual(["markdown"]);
	});

	test("unticks one that was", () => {
		expect(flipped(["markdown"], "markdown")).toEqual([]);
	});
});

describe("sized", () => {
	test("says the scenes and the words", () => {
		expect(sized({ scenes: 400, words: 464941 })).toBe(
			"400 scenes · 464,941 words",
		);
	});

	test("does not make one of either plural", () => {
		expect(sized({ scenes: 1, words: 1 })).toBe("1 scene · 1 word");
	});
});

describe("wrote", () => {
	test("names the file and the folder", () => {
		expect(wrote(["Ithaca.md"], "/home/me/Exports")).toBe(
			"Wrote Ithaca.md to /home/me/Exports.",
		);
	});
});
