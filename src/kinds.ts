// What a + may make, wherever it sits. Rust enforces the same rules in
// `tree::may_hold` and refuses anything else; this is only so the writer is
// never offered something that would be turned down.

import type { FolderKind } from "./types";

/**
 * The name of the one section with a hierarchy of its own. Rust holds the same
 * name in `project::MANUSCRIPT`; a folder in any other section is a folder and
 * nothing more.
 */
export const MANUSCRIPT = "Manuscript";

/** One thing a + offers. */
export type Making = {
	/** Which command makes it. */
	what: "document" | "folder";
	/** What the folder will be. Always null for a document. */
	kind: FolderKind | null;
	/** As the menu offers it. */
	label: string;
	/** As the field asks for it: "Name of the new part in Manuscript". */
	noun: string;
	placeholder: string;
};

/**
 * Every folder offers a document. What it is called depends on where it sits:
 * a document anywhere in the Manuscript is a scene, however deep, and a
 * document in any other section is only a document.
 */
const DOCUMENT: Making = {
	what: "document",
	kind: null,
	label: "New document",
	noun: "document",
	placeholder: "Ideas",
};

const SCENE: Making = {
	...DOCUMENT,
	label: "New scene",
	noun: "scene",
	placeholder: "Scene 2",
};

const PART: Making = {
	what: "folder",
	kind: "part",
	label: "New part",
	noun: "part",
	placeholder: "Part One",
};

const CHAPTER: Making = {
	what: "folder",
	kind: "chapter",
	label: "New chapter",
	noun: "chapter",
	placeholder: "Chapter 2",
};

const FOLDER: Making = {
	what: "folder",
	kind: null,
	label: "New folder",
	noun: "folder",
	placeholder: "Research",
};

/**
 * What an empty field suggests for a folder of this kind. The + knows what it
 * is about to make; a rename has only the folder in front of it to go on.
 */
export function folderPlaceholder(kind: FolderKind | null): string {
	switch (kind) {
		case "part":
			return PART.placeholder;
		case "chapter":
			return CHAPTER.placeholder;
		case null:
			return FOLDER.placeholder;
	}
}

/**
 * Whether a folder of this kind may sit inside a folder that is itself
 * `parent`, where `inManuscript` says whether that folder is the Manuscript or
 * sits somewhere under it. Rust holds the same rules in `tree::may_hold` and
 * refuses anything else, whether it is being made there or moved there.
 *
 * The Manuscript is the only place a folder has a kind at all. A part goes
 * directly in it, a chapter goes in it or in a part, and a chapter holds
 * documents rather than folders. Everywhere else a folder is just a folder, at
 * any depth.
 */
export function mayHold(
	inManuscript: boolean,
	parent: FolderKind | null,
	kind: FolderKind | null,
): boolean {
	if (!inManuscript) {
		return kind === null;
	}

	switch (parent) {
		case null:
			return kind !== null;
		case "part":
			return kind === "chapter";
		case "chapter":
			return false;
	}
}

/**
 * What may be made inside a folder. `kind` is that folder's own kind and
 * `section` is the top-level folder it lives under.
 *
 * A document may sit at any level, so it is always on offer, called a scene in
 * the Manuscript and a document elsewhere. Which folders follow it is
 * `mayHold`, so the + and a move answer to the same rules.
 */
export function creatable(kind: FolderKind | null, section: string): Making[] {
	const inManuscript = section === MANUSCRIPT;

	return [
		inManuscript ? SCENE : DOCUMENT,
		...[PART, CHAPTER, FOLDER].filter((making) =>
			mayHold(inManuscript, kind, making.kind),
		),
	];
}
