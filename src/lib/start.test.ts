import { describe, expect, it } from "vitest";
import type { StructureLayout } from "../types";
import { counts, draftOf, read } from "./start";

const threeActs: StructureLayout = {
	structure: "three-acts",
	partNames: ["Act One", "Act Two", "Act Three"],
	parts: 3,
	chapters: 24,
	scenes: 2,
};

const yourOwn: StructureLayout = {
	structure: "your-own",
	partNames: [],
	parts: 3,
	chapters: 18,
	scenes: 2,
};

describe("counts", () => {
	it("takes a layout's defaults as they are", () => {
		expect(counts(draftOf(threeActs), threeActs)).toEqual({
			parts: 3,
			chapters: 24,
			scenes: 2,
		});
	});

	it("keeps to the ranges at both ends", () => {
		const draft = { parts: "", chapters: "3", scenes: "0" };
		expect(counts(draft, threeActs)).toEqual({
			parts: 3,
			chapters: 3,
			scenes: 0,
		});
		expect(
			counts({ ...draft, chapters: "99", scenes: "20" }, threeActs),
		).not.toBeNull();
		expect(counts({ ...draft, chapters: "2" }, threeActs)).toBeNull();
		expect(counts({ ...draft, chapters: "100" }, threeActs)).toBeNull();
		expect(counts({ ...draft, scenes: "21" }, threeActs)).toBeNull();
	});

	it("refuses what is not a whole number", () => {
		const draft = draftOf(threeActs);
		for (const text of ["", " ", "-1", "2.5", "1e2", "two"]) {
			expect(counts({ ...draft, chapters: text }, threeActs)).toBeNull();
		}
	});

	it("reads parts from the draft for Your own only", () => {
		const draft = { parts: "4", chapters: "10", scenes: "0" };
		expect(counts(draft, yourOwn)).toEqual({
			parts: 4,
			chapters: 10,
			scenes: 0,
		});
		expect(counts({ ...draft, parts: "11" }, yourOwn)).toBeNull();
		expect(counts({ ...draft, parts: "0" }, yourOwn)).toBeNull();
		expect(counts({ ...draft, parts: "11" }, threeActs)?.parts).toBe(3);
	});
});

describe("read", () => {
	it("marks chapters out when there are fewer than parts", () => {
		const fields = read(
			{ parts: "5", chapters: "4", scenes: "2" },
			yourOwn,
		);
		expect(fields).toEqual({ parts: 5, chapters: null, scenes: 2 });
	});
});
