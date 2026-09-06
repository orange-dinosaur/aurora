// Everyone on the book besides the author. A contributor is held by its place
// in the list rather than by its name: two rows can be half typed, blank, or
// the same person in two roles, and none of those should be the same entry.

import type { Contributor, Role } from "./types";

/**
 * The roles Aurora offers, in the order the picker lists them: the three kinds
 * of editing first, since a book has more of those than anything else, then the
 * people who make it something other than words.
 */
export const ROLES: Role[] = [
	"editor",
	"copyEditor",
	"proofreader",
	"coverDesigner",
	"illustrator",
	"translator",
	"narrator",
];

/** What the picker writes for a role. The tag is stored; this is never. */
const NAMES: Record<Role, string> = {
	editor: "Editor",
	copyEditor: "Copy editor",
	proofreader: "Proofreader",
	coverDesigner: "Cover designer",
	illustrator: "Illustrator",
	translator: "Translator",
	narrator: "Narrator",
};

export function titled(role: Role): string {
	return NAMES[role];
}

/** An empty row at the foot, waiting to be filled in. */
export function added(list: Contributor[]): Contributor[] {
	return [...list, { name: "", role: "editor" }];
}

export function removed(list: Contributor[], at: number): Contributor[] {
	return list.filter((_, index) => index !== at);
}

/** Puts a changed row back where it was, which is what both boxes write. */
export function changed(
	list: Contributor[],
	at: number,
	person: Contributor,
): Contributor[] {
	return list.map((was, index) => (index === at ? person : was));
}

/**
 * What is worth writing down. A row with no name is one the writer opened and
 * has not filled in, and it should not reach `aurora.json` as a nameless
 * editor.
 */
export function kept(list: Contributor[]): Contributor[] {
	return list
		.map((person) => ({ ...person, name: person.name.trim() }))
		.filter((person) => person.name !== "");
}
