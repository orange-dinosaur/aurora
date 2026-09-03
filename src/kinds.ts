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
 * What may be made inside a folder. `kind` is that folder's own kind and
 * `section` is the top-level folder it lives under.
 *
 * A document may sit at any level, so it is always on offer. The folders are
 * the Manuscript's rules: a part directly in the Manuscript, a chapter in the
 * Manuscript or in a part, nothing inside a chapter, and no kind at all
 * anywhere else.
 */
export function creatable(kind: FolderKind | null, section: string): Making[] {
	if (section !== MANUSCRIPT) {
		return [DOCUMENT, FOLDER];
	}

	switch (kind) {
		case null:
			return [SCENE, PART, CHAPTER];
		case "part":
			return [SCENE, CHAPTER];
		case "chapter":
			return [SCENE];
	}
}
