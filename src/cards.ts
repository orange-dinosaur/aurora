// The lines a folder's overview writes on its cards and under its title. They
// live apart from the view because they are the part worth checking: the rest
// of the overview is a shape, and the writer looks at that themselves.

import type { FolderKind, OverviewCard } from "./types";

/** A document card's count, which says what it is aiming at if it is. */
export function counted(words: number, target: number | null) {
	if (target !== null) {
		return `${words.toLocaleString()} of ${target.toLocaleString()} words`;
	}
	return words === 1 ? "1 word" : `${words.toLocaleString()} words`;
}

/**
 * What a folder card says it is. A folder outside the Manuscript is only ever
 * a folder, so it says so rather than naming a kind it does not have.
 */
export function described(kind: FolderKind | null, children: number) {
	const what =
		kind === null ? "Folder" : kind === "part" ? "Part" : "Chapter";
	const held = children === 1 ? "1 item" : `${children} items`;
	return `${what} · ${held}`;
}

/**
 * What the folder amounts to, for the line under its name. Only what it holds
 * directly: reaching through the folders below it would mean reading every
 * file under them.
 */
export function summarised(cards: OverviewCard[]) {
	let words = 0;
	let documents = 0;
	for (const card of cards) {
		if (card.node === "document") {
			words += card.words;
			documents += 1;
		}
	}
	const kept = documents === 1 ? "1 document" : `${documents} documents`;
	return `${kept} · ${words.toLocaleString()} words`;
}
