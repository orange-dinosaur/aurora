// The tabs a project was left with, turned into what the store keeps and back.
// The store holds ids only, so a tab whose document or folder has gone since
// has nothing to come back as and is dropped.

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

export function remembered(
	tabs: (Reopened & { key: string })[],
	activeKey: string | null,
): Workspace {
	const active = tabs.findIndex((tab) => tab.key === activeKey);
	return {
		tabs: tabs.map((tab): SavedTab => {
			switch (tab.kind) {
				case "document":
					return {
						kind: "document",
						id: tab.document.id,
						caret: null,
						scroll: 0,
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
