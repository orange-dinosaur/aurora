import { describe, expect, test } from "vitest";
import type { Workspace } from "./bindings/Workspace";
import type { TreeNode } from "./types";
import { remembered, reopen } from "./workspace";

function folder(name: string, children: TreeNode[] = []): TreeNode {
	return {
		node: "folder",
		id: `id-${name}`,
		name,
		kind: null,
		children,
		words: 0,
		inBook: true,
	};
}

function document(path: string): TreeNode {
	const name = path.slice(path.lastIndexOf("/") + 1);
	return {
		node: "document",
		id: `id-${path}`,
		path,
		trail: path.split("/").slice(0, -1),
		title: name.replace(/\.md$/, ""),
		target: null,
		inBook: true,
	};
}

const tree = [
	folder("Manuscript", [
		folder("Chapter 1", [document("Manuscript/Chapter 1/Scene 1.md")]),
	]),
	folder("Characters", [document("Characters/Elena.md")]),
];

function saved(
	tabs: Workspace["tabs"],
	active: number | null = null,
): Workspace {
	return { tabs, active };
}

describe("reopen", () => {
	test("brings back every kind of tab in its order", () => {
		const { tabs, active } = reopen(
			saved(
				[
					{
						kind: "document",
						id: "id-Manuscript/Chapter 1/Scene 1.md",
						caret: null,
						scroll: 0,
					},
					{ kind: "folder", id: "id-Chapter 1" },
					{ kind: "book" },
					{ kind: "trash" },
					{ kind: "search" },
					{ kind: "tag", tag: "harbour" },
					{ kind: "subject", id: "id-Characters/Elena.md" },
				],
				1,
			),
			tree,
		);

		expect(tabs.map((tab) => tab.kind)).toEqual([
			"document",
			"folder",
			"book",
			"trash",
			"search",
			"tag",
			"subject",
		]);
		expect(active).toBe(1);
	});

	test("rebuilds a folder the way the sidebar opens it", () => {
		const { tabs } = reopen(
			saved([{ kind: "folder", id: "id-Chapter 1" }]),
			tree,
		);

		expect(tabs[0]).toEqual({
			kind: "folder",
			folder: {
				id: "id-Chapter 1",
				name: "Chapter 1",
				kind: null,
				section: "Manuscript",
				trail: ["Manuscript", "Chapter 1"],
			},
		});
	});

	test("drops tabs whose document or folder has gone", () => {
		const { tabs, active } = reopen(
			saved(
				[
					{ kind: "book" },
					{ kind: "document", id: "id-gone", caret: null, scroll: 0 },
					{ kind: "folder", id: "id-gone" },
					{ kind: "subject", id: "id-gone" },
					{ kind: "trash" },
				],
				4,
			),
			tree,
		);

		expect(tabs.map((tab) => tab.kind)).toEqual(["book", "trash"]);
		expect(active).toBe(1);
	});

	test("puts the tab before a gone front tab in front", () => {
		const { active } = reopen(
			saved(
				[
					{ kind: "book" },
					{ kind: "document", id: "id-gone", caret: null, scroll: 0 },
					{ kind: "trash" },
				],
				1,
			),
			tree,
		);

		expect(active).toBe(0);
	});

	test("puts the first tab left in front when nothing came before", () => {
		const { active } = reopen(
			saved(
				[
					{ kind: "document", id: "id-gone", caret: null, scroll: 0 },
					{ kind: "trash" },
				],
				0,
			),
			tree,
		);

		expect(active).toBe(0);
	});

	test("has nothing in front when every tab has gone", () => {
		const { tabs, active } = reopen(
			saved([{ kind: "folder", id: "id-gone" }], 0),
			tree,
		);

		expect(tabs).toEqual([]);
		expect(active).toBeNull();
	});
});

describe("remembered", () => {
	test("goes back through reopen unchanged", () => {
		const { tabs } = reopen(
			saved([
				{ kind: "folder", id: "id-Chapter 1" },
				{ kind: "tag", tag: "harbour" },
				{ kind: "subject", id: "id-Characters/Elena.md" },
			]),
			tree,
		);
		const keyed = tabs.map((tab, index) => ({
			...tab,
			key: String(index),
		}));

		expect(remembered(keyed, "2")).toEqual(
			saved(
				[
					{ kind: "folder", id: "id-Chapter 1" },
					{ kind: "tag", tag: "harbour" },
					{ kind: "subject", id: "id-Characters/Elena.md" },
				],
				2,
			),
		);
	});

	test("has nothing in front when no tab is active", () => {
		expect(
			remembered([{ kind: "book", key: "1" }], null).active,
		).toBeNull();
	});
});
