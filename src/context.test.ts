import { describe, expect, test } from "vitest";
import { contextOf } from "./context";

/** A line built so the match sits at a known offset in known text. */
function around(before: string, match: string, after: string) {
	const line = before + match + after;
	return contextOf(line, before.length, before.length + match.length);
}

/** Words, so a cut has somewhere to land. */
function words(count: number): string {
	return Array.from({ length: count }, (_, i) => `word${i}`).join(" ");
}

describe("a short block", () => {
	test("is shown whole, with no ellipses", () => {
		expect(around("She saw the ", "wren", " on the sill.")).toEqual({
			before: "She saw the ",
			match: "wren",
			after: " on the sill.",
		});
	});

	test("keeps a match at the very start", () => {
		expect(around("", "Wren", " went down.")).toEqual({
			before: "",
			match: "Wren",
			after: " went down.",
		});
	});

	test("keeps a match at the very end", () => {
		expect(around("She saw the ", "wren", "")).toEqual({
			before: "She saw the ",
			match: "wren",
			after: "",
		});
	});
});

describe("a long block", () => {
	test("is cut on both sides", () => {
		const { before, match, after } = around(
			`${words(30)} `,
			"wren",
			` ${words(30)}`,
		);

		expect(before.startsWith("…")).toBe(true);
		expect(after.endsWith("…")).toBe(true);
		expect(match).toBe("wren");
	});

	test("cuts at a space, not through a word", () => {
		const { before, after } = around(
			`${words(30)} `,
			"wren",
			` ${words(30)}`,
		);

		expect(before.slice(1)).toMatch(/^word\d+ /);
		expect(after.slice(0, -1)).toMatch(/ word\d+$/);
	});

	test("keeps roughly the same amount either side", () => {
		const { before, after } = around(
			`${words(30)} `,
			"wren",
			` ${words(30)}`,
		);

		expect(before.length).toBeGreaterThan(30);
		expect(before.length).toBeLessThan(55);
		expect(after.length).toBeGreaterThan(30);
		expect(after.length).toBeLessThan(55);
	});

	test("cuts through a word rather than losing the context to one long one", () => {
		const wall = "x".repeat(200);
		const { before, after } = around(wall, "wren", wall);

		expect(before).toBe(`…${"x".repeat(40)}`);
		expect(after).toBe(`${"x".repeat(40)}…`);
	});
});

describe("the match itself", () => {
	test("survives whole however long the query is", () => {
		const phrase = words(40);
		const { match } = around(`${words(20)} `, phrase, ` ${words(20)}`);

		expect(match).toBe(phrase);
	});
});
