// The Book page's fields, gathered under headings. Eleven rows in one column
// is a wall, so each group folds; a folded one says how much it is holding so
// that nothing the writer filled in disappears without trace.

import { kept } from "./contributors";
import type { Book } from "./types";

/**
 * What the store records. A key rather than the heading, so a heading can be
 * reworded without every writer's folded groups springing open.
 */
export type GroupKey = "identity" | "people" | "itself" | "publication";

export type Group = {
	key: GroupKey;
	name: string;
};

export const GROUPS: Group[] = [
	{ key: "identity", name: "Identity" },
	{ key: "people", name: "People" },
	{ key: "itself", name: "The book itself" },
	{ key: "publication", name: "Publication" },
];

/** How many of a group's fields the writer has said something in. */
export function filled(book: Book, key: GroupKey): number {
	switch (key) {
		case "identity":
			return said([
				book.title,
				book.subtitle,
				book.series,
				book.seriesNumber,
				book.language,
			]);
		case "people":
			return said([book.author]) + kept(book.contributors).length;
		case "itself":
			return said([book.blurb]) + book.keywords.length;
		case "publication":
			return said([
				book.publisher,
				book.publicationDate,
				book.isbn,
				book.copyright,
			]);
	}
}

function said(values: string[]): number {
	return values.filter((value) => value.trim() !== "").length;
}

/** Folds an open group and opens a folded one. */
export function toggled(shut: GroupKey[], key: GroupKey): GroupKey[] {
	return shut.includes(key)
		? shut.filter((each) => each !== key)
		: [...shut, key];
}

/**
 * The keys worth acting on out of whatever the store hands back. A group that
 * was renamed away, or a line the writer edited into the file themselves, is
 * not a group any more.
 */
export function known(shut: string[]): GroupKey[] {
	return GROUPS.filter((group) => shut.includes(group.key)).map(
		(group) => group.key,
	);
}
