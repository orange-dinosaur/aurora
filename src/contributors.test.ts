import { describe, expect, test } from "vitest";
import { added, changed, kept, removed, titled } from "./contributors";
import type { Contributor } from "./types";

const three: Contributor[] = [
	{ name: "Homer", role: "editor" },
	{ name: "Chapman", role: "translator" },
	{ name: "Flaxman", role: "illustrator" },
];

describe("added", () => {
	test("opens a blank row at the foot", () => {
		expect(added(three)).toHaveLength(4);
		expect(added(three)[3]).toEqual({ name: "", role: "editor" });
	});

	test("leaves the list it was given alone", () => {
		added(three);
		expect(three).toHaveLength(3);
	});
});

describe("removed", () => {
	test("takes the one asked for and keeps the order", () => {
		expect(removed(three, 1).map((person) => person.name)).toEqual([
			"Homer",
			"Flaxman",
		]);
	});

	test("goes by place, not by name", () => {
		const twice: Contributor[] = [
			{ name: "Chapman", role: "translator" },
			{ name: "Chapman", role: "editor" },
		];

		expect(removed(twice, 0)).toEqual([
			{ name: "Chapman", role: "editor" },
		]);
	});
});

describe("changed", () => {
	test("puts the row back where it was", () => {
		const next = changed(three, 1, { name: "Pope", role: "translator" });

		expect(next[1]).toEqual({ name: "Pope", role: "translator" });
		expect(next.map((person) => person.name)).toEqual([
			"Homer",
			"Pope",
			"Flaxman",
		]);
	});
});

describe("kept", () => {
	test("drops a row nobody was named in", () => {
		expect(kept([...three, { name: "  ", role: "narrator" }])).toEqual(
			three,
		);
	});

	test("trims what is written down without touching the role", () => {
		expect(kept([{ name: " Homer ", role: "narrator" }])).toEqual([
			{ name: "Homer", role: "narrator" },
		]);
	});

	test("keeps three in the order they were entered", () => {
		expect(kept(three).map((person) => person.name)).toEqual([
			"Homer",
			"Chapman",
			"Flaxman",
		]);
	});
});

describe("titled", () => {
	test("names a role for the picker", () => {
		expect(titled("illustrator")).toBe("Illustrator");
	});

	test("writes a two-word role as two words", () => {
		expect(titled("coverDesigner")).toBe("Cover designer");
	});
});
