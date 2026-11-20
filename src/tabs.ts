// Pure work on the strip of tabs, kept apart from the component so it can be
// tested.

import type { TakeView } from "./bindings/TakeView";
import type { Seed } from "./lib/find";
import type { TabView } from "./Tabs";
import { documentsOf, folderRef, type FolderRef } from "./tree";
import type { ProjectDocument, TreeNode } from "./types";

export type Content =
	| { kind: "loading" }
	| { kind: "ready"; text: string }
	| { kind: "error"; message: string };

export type Save =
	| { kind: "clean" }
	| { kind: "pending" }
	| { kind: "saving" }
	// The file has gone from under the tab, so the text here is all there is
	// left of it.
	| { kind: "missing" }
	// Something else wrote the file since this tab read it. Nothing is saved
	// until the writer picks the file's text or their own.
	| { kind: "changed" }
	| { kind: "failed"; message: string };

export type DocumentTab = {
	kind: "document";
	key: string;
	document: ProjectDocument;
	content: Content;
	save: Save;
	/** What search asked Find to open on here, or null if nothing did. */
	seed: Seed | null;
	/** Here rather than in the strip, so one mounted again keeps its line. */
	/** Null when the list could not be read. */
	takes: TakeView[] | null;
};

export type FolderTab = {
	kind: "folder";
	key: string;
	/** The folder the overview is of, and what may be made inside it. */
	folder: FolderRef;
};

export type BookTab = {
	kind: "book";
	key: string;
};

export type TrashTab = {
	kind: "trash";
	key: string;
};

export type TagTab = {
	kind: "tag";
	key: string;
	tag: string;
};

export type SubjectTab = {
	kind: "subject";
	key: string;
	/** The subject's own document, which the page is a reading of. */
	subject: ProjectDocument;
};

export type Tab =
	DocumentTab | FolderTab | BookTab | TrashTab | TagTab | SubjectTab;

/**
 * One tab as the strip shows it. The kind travels with it: a folder called
 * `Notes` and a document called `Notes` are not the same tab, and the strip
 * used to have only the missing `folder/` prefix to tell them apart. Only a
 * document can be unsaved.
 */
export function strip(tab: Tab): TabView {
	switch (tab.kind) {
		case "document":
			return {
				key: tab.key,
				kind: "document",
				// A tab has room for the folder holding it, not the whole trail
				// down to it.
				folder: tab.document.trail.slice(-1)[0] ?? null,
				title: tab.document.title,
				dirty: tab.save.kind !== "clean",
			};
		case "folder":
			return {
				key: tab.key,
				kind: "folder",
				folder: null,
				title: tab.folder.name,
				dirty: false,
			};
		case "book":
			return {
				key: tab.key,
				kind: "book",
				folder: null,
				title: "Book",
				dirty: false,
			};
		case "trash":
			return {
				key: tab.key,
				kind: "trash",
				folder: null,
				title: "Trash",
				dirty: false,
			};
		case "tag":
			return {
				key: tab.key,
				kind: "tag",
				folder: null,
				title: tab.tag,
				dirty: false,
			};
		case "subject":
			return {
				key: tab.key,
				kind: "subject",
				folder: null,
				title: tab.subject.title,
				dirty: false,
			};
	}
}

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
