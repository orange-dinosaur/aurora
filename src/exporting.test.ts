import { describe, expect, test } from "vitest";
import { doing, flipped, fraction, sized, warned, wrote } from "./exporting";

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

describe("doing", () => {
	test("counts the scenes while reading them", () => {
		expect(
			doing({ stage: "reading", done: 120, total: 402, scenes: 401 }),
		).toBe("Reading scenes, 120 of 401");
	});

	test("names the file being written", () => {
		expect(
			doing({
				stage: "writing",
				done: 401,
				total: 402,
				name: "Ithaca.md",
			}),
		).toBe("Writing Ithaca.md");
	});
});

describe("fraction", () => {
	test("is the steps done over all of them", () => {
		expect(
			fraction({
				stage: "writing",
				done: 1,
				total: 4,
				name: "Ithaca.md",
			}),
		).toBe(0.25);
	});

	test("is nothing for an export with nothing to do", () => {
		expect(
			fraction({ stage: "reading", done: 0, total: 0, scenes: 0 }),
		).toBe(0);
	});
});

describe("wrote", () => {
	test("names the file and the folder", () => {
		expect(wrote(["Ithaca.md"], "/home/me/Exports")).toBe(
			"Wrote Ithaca.md to /home/me/Exports.",
		);
	});
});

describe("warned", () => {
	test("warns about an EPUB without a cover", () => {
		expect(warned(["markdown", "epub"], false)).toBe(
			"No cover: most libraries will show a blank tile.",
		);
	});

	test("says nothing once there is a cover, or no EPUB", () => {
		expect(warned(["epub"], true)).toBe("");
		expect(warned(["markdown"], false)).toBe("");
	});
});
