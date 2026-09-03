// The document commands, in one place. Both the sidebar and the section
// overview offer the same actions, so each command is called from here rather
// than from whichever surface the writer happened to use.

import { invoke } from "@tauri-apps/api/core";
import type {
	FolderKind,
	ProjectDocument,
	TrashEntry,
	TreeNode,
} from "./types";

/**
 * Starts a new, empty document at the end of a folder, which may be a section
 * or anything under one. The name is the title; Rust adds the extension.
 */
export function createDocument(
	root: string,
	parentId: string,
	name: string,
): Promise<ProjectDocument> {
	return invoke<ProjectDocument>("create_document", { root, parentId, name });
}

/**
 * Makes a folder at the end of another one. Rust refuses a kind that does not
 * belong where it is being put, so the caller offering only the kinds that fit
 * is a courtesy rather than the rule.
 */
export function createFolder(
	root: string,
	parentId: string,
	name: string,
	kind: FolderKind | null,
): Promise<TreeNode> {
	return invoke<TreeNode>("create_folder", { root, parentId, name, kind });
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
