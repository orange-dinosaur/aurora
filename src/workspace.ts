// The tabs a project was left with, turned into what the store keeps and back.
// The store holds ids only, so a tab whose document or folder has gone since
// has nothing to come back as and is dropped.

import type { Caret } from "./bindings/Caret";
import type { SavedTab } from "./bindings/SavedTab";
import type { Workspace } from "./bindings/Workspace";
import { documentsOf, folderRef, type FolderRef } from "./tree";
import type { ProjectDocument, TreeNode } from "./types";

/** A tab as it reopens, before it has a key or anything read from disk. */
export type Reopened =
	| { kind: "document"; document: ProjectDocument }
	| { kind: "folder"; folder: FolderRef }
	| { kind: "book" }
	| { kind: "trash" }
	| { kind: "search" }
	| { kind: "tag"; tag: string }
	| { kind: "subject"; subject: ProjectDocument };

/** Where a document was left: the caret, and how far down the page was. */
export type Place = { caret: Caret | null; scroll: number };

export function remembered(
	tabs: (Reopened & { key: string })[],
	activeKey: string | null,
	places: ReadonlyMap<string, Place> = new Map(),
): Workspace {
	const active = tabs.findIndex((tab) => tab.key === activeKey);
	return {
		tabs: tabs.map((tab): SavedTab => {
			switch (tab.kind) {
				case "document":
					return {
						kind: "document",
						id: tab.document.id,
						...(places.get(tab.document.id) ?? {
							caret: null,
							scroll: 0,
						}),
					};
				case "folder":
					return { kind: "folder", id: tab.folder.id };
				case "tag":
					return { kind: "tag", tag: tab.tag };
				case "subject":
					return { kind: "subject", id: tab.subject.id };
				default:
					return { kind: tab.kind };
			}
		}),
		active: active === -1 ? null : active,
	};
}

/**
 * The tabs that can still open, in their order, and which one goes in front.
 * When the front tab is the one that has gone, the tab before it takes its
 * place, as closing it would have done.
 */
export function reopen(
	saved: Workspace,
	tree: TreeNode[],
): { tabs: Reopened[]; active: number | null } {
	const documents = new Map(
		documentsOf(tree).map((document) => [document.id, document]),
	);
	const tabs: Reopened[] = [];
	let active: number | null = null;

	for (const [index, tab] of saved.tabs.entries()) {
		const back = reopened(tab, documents, tree);
		if (back !== null) {
			tabs.push(back);
		}
		if (index === saved.active && tabs.length > 0) {
			active = tabs.length - 1;
		}
	}

	// Nothing survived before the front tab, so the first one left is next.
	if (active === null && saved.active !== null && tabs.length > 0) {
		active = 0;
	}
	return { tabs, active };
}

/**
 * Which run of text an offset into a paragraph falls in, and how far into it,
 * given the length of each run. An offset at the seam between two runs goes
 * at the end of the first. The file may have shrunk since the caret was
 * saved, so an offset past the end lands at the end.
 */
export function locate(
	lengths: number[],
	offset: number,
): { run: number; offset: number } {
	let left = Math.max(0, offset);
	for (const [run, length] of lengths.entries()) {
		if (left <= length) {
			return { run, offset: left };
		}
		left -= length;
	}
	const last = lengths.length - 1;
	return { run: last, offset: lengths[last] ?? 0 };
}

function reopened(
	tab: SavedTab,
	documents: Map<string, ProjectDocument>,
	tree: TreeNode[],
): Reopened | null {
	switch (tab.kind) {
		case "document": {
			const document = documents.get(tab.id);
			return document === undefined
				? null
				: { kind: "document", document };
		}
		case "subject": {
			const subject = documents.get(tab.id);
			return subject === undefined ? null : { kind: "subject", subject };
		}
		case "folder": {
			const folder = folderRef(tree, tab.id);
			return folder === null ? null : { kind: "folder", folder };
		}
		case "tag":
			return { kind: "tag", tag: tab.tag };
		default:
			return { kind: tab.kind };
	}
}
