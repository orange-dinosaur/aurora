// Which books the shelf shows, and in what order: narrowed to one format, to
// a search on title and author, and sorted. The shelf stays one flat run.

import type { Format } from "./bindings/Format";
import type { RecentSummary } from "./bindings/RecentSummary";
import { folderName } from "./recents";

export type ShelfFormat = Format | "all";
export type ShelfSort = "opened" | "title" | "length";

/** About how many words a screenplay page holds, to compare it with prose. */
const WORDS_PER_PAGE = 250;

/**
 * The name a book goes by. One nobody has titled goes by its project's name.
 * One that cannot be read has only its folder: the name may be as stale as
 * the file that could not be read.
 */
export function bookTitle(book: RecentSummary): string {
	return book.words === null
		? folderName(book.root)
		: book.title?.trim() || book.name;
}

/** In words, a screenplay's pages counted as words. -1 for an unread book. */
export function bookLength(book: RecentSummary): number {
	if (book.pages !== null) {
		return book.pages * WORDS_PER_PAGE;
	}
	return book.words ?? -1;
}

/**
 * The books to show. "opened" keeps the order they came in, which is the
 * order they were last opened. A book that cannot be read has no format, so
 * it shows only under all.
 */
export function shelved(
	books: RecentSummary[],
	format: ShelfFormat,
	search: string,
	sort: ShelfSort,
): RecentSummary[] {
	const wanted = search.trim().toLocaleLowerCase();
	const shown = books.filter(
		(book) =>
			(format === "all" || book.format === format) &&
			(wanted === "" ||
				bookTitle(book).toLocaleLowerCase().includes(wanted) ||
				(book.author ?? "").toLocaleLowerCase().includes(wanted)),
	);
	if (sort === "title") {
		return shown.sort((one, other) =>
			bookTitle(one).localeCompare(bookTitle(other)),
		);
	}
	if (sort === "length") {
		return shown.sort((one, other) => bookLength(other) - bookLength(one));
	}
	return shown;
}

/** How many books each format has, and all of them under "all". */
export function formatCounts(
	books: RecentSummary[],
): Record<ShelfFormat, number> {
	const counts: Record<ShelfFormat, number> = {
		all: books.length,
		novel: 0,
		screenplay: 0,
		"short-stories": 0,
		"stage-play": 0,
	};
	for (const book of books) {
		if (book.format !== null) {
			counts[book.format] += 1;
		}
	}
	return counts;
}
