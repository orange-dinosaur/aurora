import { describe, expect, test } from "vitest";
import { subjectNames } from "./subjects";
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
