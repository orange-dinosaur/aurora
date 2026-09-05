import { describe, expect, test } from "vitest";
import type { Fields } from "./frontmatter";
import { factsOf, subjectNames } from "./subjects";
import type { TreeNode } from "./types";

function folder(name: string, children: TreeNode[]): TreeNode {
	return { node: "folder", id: name, name, kind: null, children, words: 0 };
}

function document(title: string, trail: string[]): TreeNode {
	return {
		node: "document",
		id: `${trail.join("/")}/${title}.md`,
		path: `${trail.join("/")}/${title}.md`,
		trail,
		title,
		target: null,
	};
}

describe("subjectNames", () => {
	test("takes the documents under the subject sections and no others", () => {
		const tree = [
			folder("Manuscript", [document("Chapter One", ["Manuscript"])]),
			folder("Characters", [document("Isolde", ["Characters"])]),
			folder("Locations", [document("Ithaca", ["Locations"])]),
			folder("Notes", [document("Research", ["Notes"])]),
		];

		expect(subjectNames(tree)).toEqual(["Isolde", "Ithaca"]);
	});

	test("finds a subject however deep it is filed", () => {
		const tree = [
			folder("Characters", [
				folder("Minor", [document("Bruno", ["Characters", "Minor"])]),
			]),
		];

		expect(subjectNames(tree)).toEqual(["Bruno"]);
	});

	test("offers a repeated title once", () => {
		const tree = [
			folder("Characters", [
				document("Ana", ["Characters"]),
				folder("Minor", [document("Ana", ["Characters", "Minor"])]),
			]),
		];

		expect(subjectNames(tree)).toEqual(["Ana"]);
	});

	test("an empty project offers nothing", () => {
		expect(subjectNames([])).toEqual([]);
	});
});

describe("factsOf", () => {
	const none = { mentions: 0, first: null };

	function fields(entries: [string, string][]): Fields {
		return new Map(entries);
	}

	test("a character is introduced by its role, which leaves the boxes", () => {
		const facts = factsOf(
			["Characters"],
			fields([
				["role", "the sister who stayed"],
				["age", "34"],
				["occupation", "harbourmaster"],
			]),
			{ mentions: 12, first: "Chapter One" },
		);

		expect(facts.subtitle).toBe("the sister who stayed");
		expect(facts.boxes).toEqual([
			{ label: "age", value: "34" },
			{ label: "occupation", value: "harbourmaster" },
			{ label: "Mentions", value: "12" },
		]);
	});

	test("a location is introduced by its region and says where it is first met", () => {
		const facts = factsOf(
			["Locations"],
			fields([
				["region", "the north coast"],
				["kind", "harbour town"],
			]),
			{ mentions: 5, first: "Chapter Three" },
		);

		expect(facts.subtitle).toBe("the north coast");
		expect(facts.boxes).toEqual([
			{ label: "kind", value: "harbour town" },
			{ label: "First appears", value: "Chapter Three" },
			{ label: "Mentions", value: "5" },
		]);
	});

	test("a character does not get a first appearance box", () => {
		const facts = factsOf(["Characters"], fields([]), none);

		expect(facts.boxes).toEqual([{ label: "Mentions", value: "0" }]);
	});

	test("a place nothing names yet says so rather than showing a blank", () => {
		const facts = factsOf(["Locations"], fields([]), none);

		expect(facts.boxes).toEqual([
			{ label: "First appears", value: "—" },
			{ label: "Mentions", value: "0" },
		]);
	});

	test("a field the writer left empty keeps its box", () => {
		const facts = factsOf(["Characters"], fields([["age", ""]]), none);

		expect(facts.boxes[0]).toEqual({ label: "age", value: "—" });
	});

	test("the fields Aurora has its own controls for are not boxes", () => {
		const facts = factsOf(
			["Characters"],
			fields([
				["tags", "viewpoint"],
				["remarks", "She lies about the boat."],
				["age", "34"],
			]),
			none,
		);

		expect(facts.boxes).toEqual([
			{ label: "age", value: "34" },
			{ label: "Mentions", value: "0" },
		]);
	});

	test("a subject filed deeper is still read by its section", () => {
		const facts = factsOf(
			["Locations", "Islands"],
			fields([["region", "the north coast"]]),
			none,
		);

		expect(facts.subtitle).toBe("the north coast");
		expect(facts.boxes).toEqual([
			{ label: "First appears", value: "—" },
			{ label: "Mentions", value: "0" },
		]);
	});
});
