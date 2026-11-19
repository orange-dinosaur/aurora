// Pure work on the strip of tabs, kept apart from the component so it can be
// tested.

import { documentsOf, folderRef, type FolderRef } from "./tree";
import type { ProjectDocument, TreeNode } from "./types";

/** What a tab may point at, which the tree can move or take away. */
type Pointing =
	| { key: string; kind: "document"; document: ProjectDocument }
	| { key: string; kind: "subject"; subject: ProjectDocument }
	| { key: string; kind: "folder"; folder: FolderRef }
	| { key: string; kind: string };

/**
 * The tabs pointed again at what the tree holds now, by id, so a rename or a
 * move shows on them. A tab whose document or folder is gone is dropped.
 */
export function repointed<T extends Pointing>(
	tabs: T[],
	tree: TreeNode[],
): T[] {
	const documents = new Map(documentsOf(tree).map((doc) => [doc.id, doc]));
	return tabs.flatMap((tab): T[] => {
		if ("document" in tab) {
			const document = documents.get(tab.document.id);
			return document === undefined ? [] : [{ ...tab, document }];
		}
		if ("subject" in tab) {
			const subject = documents.get(tab.subject.id);
			return subject === undefined ? [] : [{ ...tab, subject }];
		}
		if ("folder" in tab) {
			const folder = folderRef(tree, tab.folder.id);
			return folder === null ? [] : [{ ...tab, folder }];
		}
		return [tab];
	});
}

/** The tabs without the ones whose keys are given, in their order. */
export function without<T extends { key: string }>(
	tabs: T[],
	keys: Iterable<string>,
): T[] {
	const gone = new Set(keys);
	return tabs.filter((tab) => !gone.has(tab.key));
}
