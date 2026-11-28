// What a + may make, wherever it sits. Rust enforces the same rules in
// `tree::may_hold` and refuses anything else; this is only so the writer is
// never offered something that would be turned down.

import type { FolderKind, Format } from "./types";

/**
 * The name of the one section with a hierarchy of its own. Rust holds the same
 * name in `project::MANUSCRIPT`; a folder in any other section is a folder and
 * nothing more.
 */
export const MANUSCRIPT = "Manuscript";

/**
 * What the two matter folders are called when Aurora makes one. Rust holds the
 * same names in `project::FRONT_MATTER` and `project::BACK_MATTER`, where they
 * are also what an adopted folder is recognised by. Once a folder exists its
 * kind is what it is, so the writer may rename it to anything.
 */
export const FRONT_MATTER = "Front Matter";
export const BACK_MATTER = "Back Matter";

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
	/**
	 * The name it is made under, when the whole point of the entry is that the
	 * writer does not type one. Everything else opens the name field instead.
	 */
	name?: string;
};

/**
 * The name a new thing's field opens with: "Scene 4" in a folder holding three.
 * It is selected, so typing replaces it.
 */
export function suggested(noun: string, holds: number) {
	return `${noun[0].toUpperCase()}${noun.slice(1)} ${holds + 1}`;
}

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

// A part is what a screenplay calls an act and a collection calls a story.
const ACT: Making = {
	...PART,
	label: "New act",
	noun: "act",
	placeholder: "Act Two",
};

const STORY: Making = {
	...PART,
	label: "New story",
	noun: "story",
	placeholder: "Story 2",
};

function partOf(format: Format): Making {
	switch (format) {
		case "screenplay":
			return ACT;
		case "short-stories":
			return STORY;
		default:
			return PART;
	}
}

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

// The two the + spells for the writer. A folder of either kind is made under
// the name Aurora gives it and renamed afterwards if the writer wants
// something else, because the kind is what makes it matter.
const FRONT: Making = {
	what: "folder",
	kind: "front-matter",
	label: "New front matter",
	noun: "front matter",
	placeholder: FRONT_MATTER,
	name: FRONT_MATTER,
};

const BACK: Making = {
	what: "folder",
	kind: "back-matter",
	label: "New back matter",
	noun: "back matter",
	placeholder: BACK_MATTER,
	name: BACK_MATTER,
};

/** Whether a kind is one of the two that surround the story. */
export function isMatter(kind: FolderKind | null): boolean {
	return kind === "front-matter" || kind === "back-matter";
}

/**
 * The kinds of the folders in a list of nodes, which is what `creatable` needs
 * to know whether the matter it would offer is already there. Written against
 * what a tree node and an overview card have in common, since a + is drawn over
 * both.
 */
export function folderKinds(
	nodes: readonly (
		{ node: "folder"; kind: FolderKind | null } | { node: "document" }
	)[],
): (FolderKind | null)[] {
	return nodes.flatMap((node) => (node.node === "folder" ? [node.kind] : []));
}

/**
 * What an empty field suggests for a folder of this kind. The + knows what it
 * is about to make; a rename has only the folder in front of it to go on.
 */
export function folderPlaceholder(
	kind: FolderKind | null,
	format: Format = "novel",
): string {
	switch (kind) {
		case "part":
			return partOf(format).placeholder;
		case "chapter":
			return CHAPTER.placeholder;
		case "front-matter":
			return FRONT_MATTER;
		case "back-matter":
			return BACK_MATTER;
		case null:
			return FOLDER.placeholder;
	}
}

/**
 * What an empty rename field suggests for a document in this section: a scene
 * in the Manuscript, a plain document anywhere else.
 */
export function documentPlaceholder(section: string | undefined): string {
	return (section === MANUSCRIPT ? SCENE : DOCUMENT).placeholder;
}

/**
 * Whether a folder of this kind may sit inside a folder that is itself
 * `parent`, where `inManuscript` says whether that folder is the Manuscript or
 * sits somewhere under it. Rust holds the same rules in `tree::may_hold` and
 * refuses anything else, whether it is being made there or moved there.
 *
 * The Manuscript is the only place a folder has a kind at all. A part goes
 * directly in it, a chapter goes in it or in a part, and a chapter holds
 * documents rather than folders. The front and back matter go directly in it
 * too and hold documents the way a chapter does. Everywhere else a folder is
 * just a folder, at any depth.
 *
 * A screenplay is narrower: its Manuscript holds acts, which are parts, and an
 * act holds scenes and no folder at all. A collection follows the novel.
 */
export function mayHold(
	format: Format,
	inManuscript: boolean,
	parent: FolderKind | null,
	kind: FolderKind | null,
): boolean {
	if (!inManuscript) {
		return kind === null;
	}

	const screenplay = format === "screenplay";
	switch (parent) {
		case null:
			return screenplay ? kind === "part" : kind !== null;
		case "part":
			return !screenplay && kind === "chapter";
		case "chapter":
		case "front-matter":
		case "back-matter":
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
 *
 * `held` is what the folder already holds. A Manuscript keeps one front matter
 * and one back matter, so the entry goes once there is one; Rust refuses a
 * second either way, and this is what stops the writer being offered it.
 */
export function creatable(
	kind: FolderKind | null,
	section: string,
	held: readonly (FolderKind | null)[] = [],
	format: Format = "novel",
): Making[] {
	const inManuscript = section === MANUSCRIPT;

	return [
		inManuscript ? SCENE : DOCUMENT,
		...[partOf(format), CHAPTER, FRONT, BACK, FOLDER].filter(
			(making) =>
				mayHold(format, inManuscript, kind, making.kind) &&
				!(isMatter(making.kind) && held.includes(making.kind)),
		),
	];
}
