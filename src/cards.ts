// The lines a folder's overview writes on its cards and under its title. They
// live apart from the view because they are the part worth checking: the rest
// of the overview is a shape, and the writer looks at that themselves.

import { parse, text } from "./frontmatter";
import { isMatter } from "./kinds";
import type { FolderKind, Format, OverviewCard } from "./types";

/**
 * A length in pages, in eighths the way a script breakdown writes it: "1 3/8".
 * The eighths stay unreduced, so 2/8 is never 1/4, and anything written rounds
 * up to at least 1/8.
 */
export function eighths(pages: number): string {
	const total = pages > 0 ? Math.max(1, Math.round(pages * 8)) : 0;
	const whole = Math.floor(total / 8);
	const part = total % 8;
	if (part === 0) {
		return whole.toLocaleString();
	}
	return whole === 0 ? `${part}/8` : `${whole.toLocaleString()} ${part}/8`;
}

/** "3/8 page", "1 page", "1 3/8 pages". */
export function paged(pages: number): string {
	const length = eighths(pages);
	return Math.round(pages * 8) <= 8 && length !== "0"
		? `${length} page`
		: `${length} pages`;
}

/**
 * What a count is in: pages when Rust sent them, which it does for a
 * screenplay's Manuscript, and words everywhere else.
 */
export function unit(pages: number | null): "pages" | "words" {
	return pages === null ? "words" : "pages";
}

/**
 * A card's count, which says what it is aiming at if it is. In a screenplay's
 * Manuscript the count and the target are both in pages.
 */
export function counted(
	words: number,
	target: number | null,
	pages: number | null = null,
) {
	if (pages !== null) {
		return target === null
			? paged(pages)
			: `${eighths(pages)} of ${target.toLocaleString()} pages`;
	}
	if (target !== null) {
		return `${words.toLocaleString()} of ${target.toLocaleString()} words`;
	}
	return words === 1 ? "1 word" : `${words.toLocaleString()} words`;
}

/**
 * How far along its target a row is, from 0 to 1, for the line under it.
 * Writing past the target stays at 1: the line is full, not overflowing. Null
 * when there is no target, so no line is drawn.
 */
export function reached(words: number, target: number | null) {
	if (target === null || target <= 0) {
		return null;
	}
	return Math.min(words / target, 1);
}

export type Band = "red" | "orange" | "yellow" | "green";

/** The colour a progress line takes from its share of the target. */
export function band(share: number): Band {
	if (share >= 1) {
		return "green";
	}
	if (share >= 0.5) {
		return "yellow";
	}
	return share >= 0.25 ? "orange" : "red";
}

const KIND_NAMES: Record<FolderKind, string> = {
	part: "Part",
	chapter: "Chapter",
	"front-matter": "Front matter",
	"back-matter": "Back matter",
};

// What a part is called in the formats that call it something else.
const PART_NAMES: Partial<Record<Format, string>> = {
	screenplay: "Act",
	"short-stories": "Story",
};

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
	target: number | null = null,
	format: Format = "novel",
	pages: number | null = null,
) {
	const what =
		kind === null
			? "Folder"
			: ((kind === "part" ? PART_NAMES[format] : undefined) ??
				KIND_NAMES[kind]);
	const held = children === 1 ? "1 item" : `${children} items`;
	// With a target the words get a line of their own, the one a document
	// card gives them, so they are not said twice.
	return target === null
		? `${what} · ${held} · ${counted(words, null, pages)}`
		: `${what} · ${held}`;
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
 * What the folder amounts to, for the line under its name. The documents are
 * the ones sitting in it, since a folder below it is not one; the words are
 * the whole of what is written under it, which every card now carries. Front
 * and back matter are left out of the words, as they are in the sidebar.
 *
 * The two halves come back apart because the words half is also where the
 * folder's target is set.
 */
export function summarised(cards: OverviewCard[]) {
	let words = 0;
	let pages: number | null = null;
	let documents = 0;
	for (const card of cards) {
		if (card.node === "folder" && isMatter(card.kind)) {
			continue;
		}
		words += card.words;
		if (card.pages !== null) {
			pages = (pages ?? 0) + card.pages;
		}
		if (card.node === "document") {
			documents += 1;
		}
	}
	return {
		documents: documents === 1 ? "1 document" : `${documents} documents`,
		words,
		pages,
	};
}
