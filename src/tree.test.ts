import { describe, expect, test } from "vitest";
import { documentsIn, rows, sections } from "./tree";
import type { TreeNode } from "./types";

function folder(name: string, children: TreeNode[] = []): TreeNode {
	return { node: "folder", id: `id-${name}`, name, kind: null, children };
}

/** A document written as the path it sits at, the way Rust sends it. */
function document(path: string): TreeNode {
	const name = path.slice(path.lastIndexOf("/") + 1);
	return {
		node: "document",
		id: `id-${path}`,
		path,
		folder: path.slice(0, path.indexOf("/")),
		title: name.replace(/\.md$/, ""),
		target: null,
	};
}

/** A row as `kind name depth`, which is what the sidebar draws. */
function drawn(nodes: TreeNode[]) {
	return rows(nodes, "root").map((row) =>
		row.kind === "folder"
			? `folder ${row.name} ${row.depth}`
			: `document ${row.document.title} ${row.depth}`,
	);
}

describe("laying a level out as rows", () => {
	test("a folder holding nothing has no rows", () => {
		expect(rows([], "root")).toEqual([]);
	});

	test("documents keep the order they are given", () => {
		const section = [
			document("Manuscript/Chapter 1.md"),
			document("Manuscript/Chapter 2.md"),
		];
		expect(drawn(section)).toEqual([
			"document Chapter 1 0",
			"document Chapter 2 0",
		]);
	});

	test("a folder is followed by what it holds, one level deeper", () => {
		const section = [
			folder("Part One", [
				document("Manuscript/Part One/Chapter 1.md"),
				folder("Chapter Two", [
					document("Manuscript/Part One/Chapter Two/Scene.md"),
				]),
			]),
			document("Manuscript/Epilogue.md"),
		];

		expect(drawn(section)).toEqual([
			"folder Part One 0",
			"document Chapter 1 1",
			"folder Chapter Two 1",
			"document Scene 2",
			"document Epilogue 0",
		]);
	});

	test("a row is numbered among the nodes beside it, not among all of them", () => {
		const section = [
			folder("Part One", [
				document("Manuscript/Part One/Chapter 1.md"),
				document("Manuscript/Part One/Chapter 2.md"),
			]),
			document("Manuscript/Epilogue.md"),
		];

		expect(rows(section, "root").map((row) => row.index)).toEqual([
			0, 0, 1, 1,
		]);
		expect(rows(section, "root").map((row) => row.siblings)).toEqual([
			2, 2, 2, 2,
		]);
	});

	test("a row's group is the folder holding it", () => {
		const section = [
			folder("Part One", [document("Manuscript/Part One/Chapter 1.md")]),
		];

		expect(rows(section, "root").map((row) => row.group)).toEqual([
			"root",
			"id-Part One",
		]);
	});

	test("a document row carries the view Rust sent", () => {
		const [row] = rows([document("Notes/Ideas.md")], "root");

		expect(row).toMatchObject({
			kind: "document",
			document: {
				id: "id-Notes/Ideas.md",
				path: "Notes/Ideas.md",
				folder: "Notes",
				title: "Ideas",
				target: null,
			},
		});
	});
});

describe("reading the top of the tree", () => {
	test("the sections are the folders, and a loose document is not one", () => {
		const tree = [
			folder("Manuscript"),
			document("Stray.md"),
			folder("Notes"),
		];

		expect(sections(tree).map((section) => section.name)).toEqual([
			"Manuscript",
			"Notes",
		]);
	});

	test("a section counts the documents below it, however deep they sit", () => {
		const section = [
			folder("Part One", [
				document("Manuscript/Part One/Chapter 1.md"),
				folder("Chapter Two", [
					document("Manuscript/Part One/Chapter Two/Scene.md"),
				]),
			]),
			document("Manuscript/Epilogue.md"),
		];

		expect(documentsIn(section)).toBe(3);
	});
});
