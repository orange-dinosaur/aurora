// The document commands, in one place. Both the sidebar and the section
// overview offer the same actions, so each command is called from here rather
// than from whichever surface the writer happened to use.

import { invoke } from "@tauri-apps/api/core";
import type {
	FolderKind,
	FolderNode,
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
 * Gives a folder a new name. Everything inside it comes along and keeps its
 * id, so a tab holding one of its documents needs nothing. A section is
 * refused: the Manuscript is known by its name.
 */
export function renameFolder(
	root: string,
	id: string,
	name: string,
): Promise<FolderNode> {
	return invoke<FolderNode>("rename_folder", { root, id, name });
}

/**
 * Puts a node at a given place inside a folder, which may be the one it is
 * already in: reordering and moving are the same call. An index past the end
 * means the end. Staying put touches no files; going elsewhere takes the file,
 * or the whole directory, with it.
 */
export function moveNode(
	root: string,
	id: string,
	parentId: string,
	index: number,
): Promise<void> {
	return invoke<void>("move_node", { root, id, parentId, index });
}

/** Moves a document into the project's trash. */
export function deleteDocument(root: string, id: string): Promise<void> {
	return invoke<void>("delete_document", { root, id });
}

/**
 * Moves a folder and everything in it into the trash, as one directory. The
 * caller writes down whatever is open inside it first: the files move here.
 */
export function deleteFolder(root: string, id: string): Promise<void> {
	return invoke<void>("delete_folder", { root, id });
}

/** Everything in the project's trash, most recently deleted first. */
export function listTrash(root: string): Promise<TrashEntry[]> {
	return invoke<TrashEntry[]>("list_trash", { root });
}

/**
 * Puts a deleted document or folder back where it came from, under the name it
 * had. When a folder it was inside has gone too, it lands in the nearest one
 * still standing.
 */
export function restoreFromTrash(root: string, path: string): Promise<void> {
	return invoke<void>("restore_from_trash", { root, path });
}

/** Throws one thing in the trash away for good, a folder and all with it. */
export function purgeTrashEntry(root: string, path: string): Promise<void> {
	return invoke<void>("purge_trash_entry", { root, path });
}
