import { describe, expect, test } from "vitest";
import {
	destinations,
	documentsIn,
	documentsOf,
	folded,
	folderOf,
	inside,
	rows,
	sections,
	wordsIn,
} from "./tree";
import type { Moving } from "./tree";
import type { FolderKind, TreeNode } from "./types";

function folder(
	name: string,
	children: TreeNode[] = [],
	kind: FolderKind | null = null,
): TreeNode {
	return { node: "folder", id: `id-${name}`, name, kind, children, words: 0 };
}

/** A document written as the path it sits at, the way Rust sends it. */
function document(path: string): TreeNode {
	const name = path.slice(path.lastIndexOf("/") + 1);
	return {
		node: "document",
		id: `id-${path}`,
		path,
		trail: path.split("/").slice(0, -1),
		title: name.replace(/\.md$/, ""),
		target: null,
	};
}

/** A row as `kind name depth`, which is what the sidebar draws. */
function drawn(nodes: TreeNode[], open: ReadonlySet<string> | null = null) {
	return rows(nodes, "root", open).map((row) =>
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

	test("a folder nobody has opened keeps what it holds to itself", () => {
		const section = [
			folder("Part One", [
				document("Manuscript/Part One/Chapter 1.md"),
				folder("Chapter Two", [
					document("Manuscript/Part One/Chapter Two/Scene.md"),
				]),
			]),
			document("Manuscript/Epilogue.md"),
		];

		expect(drawn(section, new Set())).toEqual([
			"folder Part One 0",
			"document Epilogue 0",
		]);

		expect(drawn(section, new Set(["id-Part One"]))).toEqual([
			"folder Part One 0",
			"document Chapter 1 1",
			"folder Chapter Two 1",
			"document Epilogue 0",
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
				trail: ["Notes"],
				title: "Ideas",
				target: null,
			},
		});
	});
});

describe("what a folder is drawn holding", () => {
	const tree = [
		folder("Research", [
			document("Notes/Research/Reading.md"),
			folder("Sources", [document("Notes/Research/Sources/Letters.md")]),
		]),
		folder("Ideas", [document("Notes/Ideas/Names.md")]),
	];

	test("everything under it, however deep, and nothing beside it", () => {
		expect([...inside(rows(tree, "root"), "id-Research")]).toEqual([
			"id-Notes/Research/Reading.md",
			"id-Sources",
			"id-Notes/Research/Sources/Letters.md",
		]);
	});

	test("a folder drawn shut holds nothing", () => {
		const shut = rows(tree, "root", new Set());
		expect(inside(shut, "id-Research").size).toBe(0);
	});

	test("a folder that is not on the list holds nothing", () => {
		expect(inside(rows(tree, "root"), "id-Sketches").size).toBe(0);
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

describe("choosing somewhere to move to", () => {
	// A Manuscript with a part, a chapter inside it and a chapter beside it,
	// plus a plain folder in another section.
	const project: TreeNode[] = [
		folder("Manuscript", [
			folder("Part One", [folder("Chapter 1", [], "chapter")], "part"),
			folder("Chapter 9", [], "chapter"),
			document("Manuscript/Prologue.md"),
		]),
		folder("Notes", [folder("Research")]),
	];

	/** Each destination as `name` or `name (no)` when it is refused. */
	function offered(moving: Moving) {
		return destinations(project, moving).map(
			(where) =>
				`${"  ".repeat(where.depth)}${where.name}${where.allowed ? "" : " (no)"}`,
		);
	}

	test("every folder is listed, in the order the tree holds them", () => {
		expect(
			offered({
				id: "id-Manuscript/Prologue.md",
				kind: null,
				folder: false,
				from: "id-Manuscript",
			}),
		).toEqual([
			"Manuscript (no)",
			"  Part One",
			"    Chapter 1",
			"  Chapter 9",
			"Notes",
			"  Research",
		]);
	});

	test("a document may go anywhere but the folder it is already in", () => {
		const where = destinations(project, {
			id: "id-Manuscript/Prologue.md",
			kind: null,
			folder: false,
			from: "id-Chapter 9",
		});

		expect(
			where.filter((one) => !one.allowed).map((one) => one.name),
		).toEqual(["Chapter 9"]);
	});

	test("a part may only go directly in the Manuscript", () => {
		expect(
			offered({
				id: "id-Part One",
				kind: "part",
				folder: true,
				from: "id-Manuscript",
			}),
		).toEqual([
			// Its own subtree is gone, and the Manuscript is where it already
			// sits.
			"Manuscript (no)",
			"  Chapter 9 (no)",
			"Notes (no)",
			"  Research (no)",
		]);
	});

	test("a chapter may go in the Manuscript or in a part", () => {
		expect(
			offered({
				id: "id-Chapter 9",
				kind: "chapter",
				folder: true,
				from: "id-Part One",
			}),
		).toEqual([
			"Manuscript",
			// Where it already is, and a chapter holds no folders.
			"  Part One (no)",
			"    Chapter 1 (no)",
			"Notes (no)",
			"  Research (no)",
		]);
	});

	test("a folder with no kind is refused by the whole Manuscript", () => {
		expect(
			offered({
				id: "id-Research",
				kind: null,
				folder: true,
				from: "id-Notes",
			}),
		).toEqual([
			"Manuscript (no)",
			"  Part One (no)",
			"    Chapter 1 (no)",
			"  Chapter 9 (no)",
			"Notes (no)",
		]);
	});

	test("a folder is never offered itself or anything it holds", () => {
		expect(
			destinations(project, {
				id: "id-Part One",
				kind: "part",
				folder: true,
				from: "id-Manuscript",
			}).map((where) => where.name),
		).not.toContain("Chapter 1");
	});

	test("a destination says how many nodes it holds, which is the end of it", () => {
		const where = destinations(project, {
			id: "id-Research",
			kind: null,
			folder: true,
			from: "id-Notes",
		});

		expect(where.find((one) => one.name === "Manuscript")?.children).toBe(
			3,
		);
	});
});

describe("every document in the tree", () => {
	test("they come back in the order a walk meets them, however deep", () => {
		const tree = [
			folder("Manuscript", [
				document("Manuscript/Scene 1.md"),
				folder("Part One", [
					document("Manuscript/Part One/Landfall.md"),
				]),
			]),
			folder("Notes", [document("Notes/Ideas.md")]),
		];

		expect(documentsOf(tree).map((doc) => doc.path)).toEqual([
			"Manuscript/Scene 1.md",
			"Manuscript/Part One/Landfall.md",
			"Notes/Ideas.md",
		]);
	});
});

describe("finding a folder to look inside", () => {
	test("it comes back with everything under it, at any depth", () => {
		const tree = [
			folder("Manuscript", [
				folder("Part One", [
					folder("Chapter 2", [
						document("Manuscript/Part One/Chapter 2/Scene.md"),
					]),
				]),
			]),
		];

		const found = folderOf(tree, "id-Chapter 2");

		expect(found?.name).toBe("Chapter 2");
		expect(documentsOf(found?.children ?? [])).toHaveLength(1);
	});

	test("an id the tree does not have, or a document's, is nothing", () => {
		const tree = [folder("Notes", [document("Notes/Ideas.md")])];

		expect(folderOf(tree, "id-Notes/Ideas.md")).toBeNull();
		expect(folderOf(tree, "id-nowhere")).toBeNull();
	});
});

describe("counting a whole project", () => {
	/** A section carrying the count Rust sends with it. */
	function section(name: string, words: number): TreeNode {
		return {
			node: "folder",
			id: `id-${name}`,
			name,
			kind: null,
			children: [],
			words,
		};
	}

	test("the project's words are its sections' words", () => {
		const tree = [section("Manuscript", 12000), section("Notes", 340)];

		expect(wordsIn(tree)).toBe(12340);
	});

	test("a project with nothing in it holds no words", () => {
		expect(wordsIn([])).toBe(0);
	});
});

describe("folding every section at once", () => {
	const tree = [folder("Manuscript"), folder("Notes")];

	test("one section open folds them all", () => {
		const open = new Set(["id-Manuscript", "id-Part One"]);

		expect([...folded(open, tree)]).toEqual(["id-Part One"]);
	});

	test("none open opens them all", () => {
		const open = new Set(["id-Part One"]);

		expect([...folded(open, tree)].sort()).toEqual([
			"id-Manuscript",
			"id-Notes",
			"id-Part One",
		]);
	});

	test("a part keeps its state through both", () => {
		const open = new Set(["id-Manuscript", "id-Part One"]);

		expect([...folded(folded(open, tree), tree)].sort()).toEqual([
			"id-Manuscript",
			"id-Notes",
			"id-Part One",
		]);
	});
});
