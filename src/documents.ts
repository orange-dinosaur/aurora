// The document commands, in one place. Both the sidebar and the section
// overview offer the same actions, so each command is called from here rather
// than from whichever surface the writer happened to use.

import { invoke } from "@tauri-apps/api/core";
import type { ProjectDocument, TrashEntry } from "./types";

/**
 * Starts a new, empty document at the end of a section. The name is the title;
 * Rust adds the extension.
 */
export function createDocument(
	root: string,
	section: string,
	name: string,
): Promise<ProjectDocument> {
	return invoke<ProjectDocument>("create_document", { root, section, name });
}

/** Gives a document a new title. It stays where it is in its section. */
export function renameDocument(
	root: string,
	id: string,
	name: string,
): Promise<ProjectDocument> {
	return invoke<ProjectDocument>("rename_document", { root, id, name });
}

/**
 * Puts a document at a given place among the others in its section. An index
 * past the end means the end.
 */
export function reorderDocument(
	root: string,
	id: string,
	index: number,
): Promise<void> {
	return invoke<void>("reorder_document", { root, id, index });
}

/** Moves a document into the project's trash. */
export function deleteDocument(root: string, id: string): Promise<void> {
	return invoke<void>("delete_document", { root, id });
}

/** Everything in the project's trash, most recently deleted first. */
export function listTrash(root: string): Promise<TrashEntry[]> {
	return invoke<TrashEntry[]>("list_trash", { root });
}

/** Puts a deleted document back where it came from, under the name it had. */
export function restoreFromTrash(
	root: string,
	path: string,
): Promise<ProjectDocument> {
	return invoke<ProjectDocument>("restore_from_trash", { root, path });
}

/** Throws one document in the trash away for good. */
export function purgeTrashEntry(root: string, path: string): Promise<void> {
	return invoke<void>("purge_trash_entry", { root, path });
}
