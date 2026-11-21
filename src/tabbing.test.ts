import { describe, expect, it } from "vitest";

import { repointed, strip, without, type Tab } from "./tabbing";
import type { ProjectDocument, TreeNode } from "./types";

function folder(id: string, name: string, children: TreeNode[]): TreeNode {
	return {
		node: "folder",
		id,
		name,
		kind: null,
		children,
		words: 0,
		target: null,
		inBook: true,
	};
}

function document(id: string, path: string): TreeNode & ProjectDocument {
	const name = path.slice(path.lastIndexOf("/") + 1);
	return {
		node: "document",
		id,
		path,
		trail: path.split("/").slice(0, -1),
		title: name.replace(/\.md$/, ""),
		target: null,
		words: 0,
		inBook: true,
	};
}

describe("repointed", () => {
	const was = [
		folder("m", "Manuscript", [
			folder("c1", "Chapter 1", [
				document("s1", "Manuscript/Chapter 1/Scene.md"),
			]),
		]),
		folder("people", "Characters", [
			document("elena", "Characters/Elena.md"),
		]),
	];
	const tabs = [
		{ key: "1", kind: "document", document: document("s1", "old") },
		{ key: "2", kind: "folder", folder: { id: "c1", name: "Chapter 1" } },
		{ key: "3", kind: "subject", subject: document("elena", "old") },
		{ key: "4", kind: "book" },
	] as const;

	it("follows a renamed folder into its tabs and its scenes'", () => {
		const now = [
			folder("m", "Manuscript", [
				folder("c1", "Arrival", [
					document("s1", "Manuscript/Arrival/Scene.md"),
				]),
			]),
			was[1],
		];
		const after = repointed([...tabs], now);
		expect(after.map((tab) => tab.key)).toEqual(["1", "2", "3", "4"]);
		expect(after[0]).toMatchObject({
			document: { path: "Manuscript/Arrival/Scene.md" },
		});
		expect(after[1]).toMatchObject({
			folder: { name: "Arrival", trail: ["Manuscript", "Arrival"] },
		});
	});

	it("follows a folder moved under another", () => {
		const now = [
			folder("m", "Manuscript", []),
			folder("people", "Characters", [
				document("elena", "Characters/Elena.md"),
				folder("c1", "Chapter 1", [
					document("s1", "Characters/Chapter 1/Scene.md"),
				]),
			]),
		];
		const after = repointed([...tabs], now);
		expect(after[1]).toMatchObject({
			folder: { section: "Characters" },
		});
	});

	it("drops the tab of a deleted subject", () => {
		const now = [was[0], folder("people", "Characters", [])];
		expect(repointed([...tabs], now).map((tab) => tab.key)).toEqual([
			"1",
			"2",
			"4",
		]);
	});
});

describe("without", () => {
	const tabs = [{ key: "1" }, { key: "2" }, { key: "3" }, { key: "4" }];

	it("drops every key given at once and keeps the order", () => {
		expect(without(tabs, ["2", "4"])).toEqual([{ key: "1" }, { key: "3" }]);
	});

	it("ignores a key no tab has", () => {
		expect(without(tabs, ["9"])).toEqual(tabs);
	});
});

describe("strip", () => {
	const scene = document("a", "Manuscript/Act one/Home.md");

	it("shows a document with its folder and whether it is unsaved", () => {
		const tab: Tab = {
			kind: "document",
			key: "1",
			document: scene,
			content: { kind: "ready", text: "" },
			save: { kind: "pending" },
			seed: null,
			takes: [],
		};
		expect(strip(tab)).toEqual({
			key: "1",
			kind: "document",
			folder: "Act one",
			title: "Home",
			dirty: true,
		});
	});

	it("shows a tag by its name and never as unsaved", () => {
		expect(strip({ kind: "tag", key: "2", tag: "elena" })).toEqual({
			key: "2",
			kind: "tag",
			folder: null,
			title: "elena",
			dirty: false,
		});
	});
});
