// The lines a folder's overview writes on its cards and under its title. They
// live apart from the view because they are the part worth checking: the rest
// of the overview is a shape, and the writer looks at that themselves.

import { parse, text } from "./frontmatter";
import { isMatter } from "./kinds";
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
 * a folder, so it says so rather than naming a kind it does not have. The
 * count beside it is what it holds directly; the words are everything under
 * it, however deep.
 */
export function described(
	kind: FolderKind | null,
	children: number,
	words: number,
) {
	const what =
		kind === null ? "Folder" : kind === "part" ? "Part" : "Chapter";
	const held = children === 1 ? "1 item" : `${children} items`;
	return `${what} · ${held} · ${counted(words, null)}`;
}

/**
 * The line under a document card's title: what the writer said the document is
 * about, and its opening only while they have not said. A synopsis is collapsed
 * onto one line the way Rust collapses an excerpt, since the card gives it four
 * lines either way and a paragraph break would spend one of them on nothing.
 */
export function previewed(front: string, excerpt: string): string {
	const synopsis = text(parse(front), "synopsis")
		.split(/\s+/)
		.filter((word) => word !== "")
		.join(" ");

	return synopsis === "" ? excerpt : synopsis;
}

/**
 * A word count for a sidebar row, where there is room for a number and not for
 * the word after it. Always short of the true figure rather than over it: a
 * writer reads their own count as a claim, and a rounded-up one is a lie.
 */
export function abbreviated(words: number) {
	if (words < 1000) {
		return String(words);
	}
	const thousands = words / 1000;
	return thousands < 10
		? `${(Math.floor(thousands * 10) / 10).toFixed(1)}k`
		: `${Math.floor(thousands)}k`;
}

/**
 * What a folder in the trash took in with it, for the line beside its name. A
 * folder that held nothing still went in as a folder and says so, or its row
 * would read as a document.
 */
export function trashed(inside: number) {
	if (inside === 0) {
		return "an empty folder";
	}
	return inside === 1
		? "a folder of 1 document"
		: `a folder of ${inside} documents`;
}

/**
 * What the folder amounts to, for the line under its name. The documents are
 * the ones sitting in it, since a folder below it is not one; the words are
 * the whole of what is written under it, which every card now carries. Front
 * and back matter are left out of the words, as they are in the sidebar.
 */
export function summarised(cards: OverviewCard[]) {
	let words = 0;
	let documents = 0;
	for (const card of cards) {
		if (card.node === "folder" && isMatter(card.kind)) {
			continue;
		}
		words += card.words;
		if (card.node === "document") {
			documents += 1;
		}
	}
	const kept = documents === 1 ? "1 document" : `${documents} documents`;
	return `${kept} · ${words.toLocaleString()} words`;
}
