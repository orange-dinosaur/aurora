// The merge screen's arithmetic: what a document reads as once its spots are
// chosen, what the binder settles to once its cards are, and how much is left
// to choose.

import type { Conflict, Piece, TreeNode } from "../types";

/** How a spot is settled: this version's words, the other's, or both. */
export type Choice = "ours" | "theirs" | "both";

/** A choice per spot, in order, null where the writer has not chosen yet. */
export type Choices = (Choice | null)[];

/**
 * The text a spot settles to. Both puts this version's words first, then the
 * other's, each as paragraphs of their own.
 */
export function settled(
	spot: { ours: string; theirs: string },
	choice: Choice,
): string {
	if (choice !== "both" || spot.theirs === "") {
		return choice === "theirs" ? spot.theirs : spot.ours;
	}
	if (spot.ours === "") {
		return spot.theirs;
	}
	const gap = spot.ours.endsWith("\n\n")
		? ""
		: spot.ours.endsWith("\n")
			? "\n"
			: "\n\n";
	return spot.ours + gap + spot.theirs;
}

/** The whole file with every spot settled, or null while one is still open. */
export function joined(pieces: Piece[], choices: Choices): string | null {
	let spot = 0;
	let text = "";
	for (const piece of pieces) {
		if (piece.kind === "clean") {
			text += piece.text;
			continue;
		}
		const choice = choices[spot] ?? null;
		spot += 1;
		if (choice === null) {
			return null;
		}
		text += settled(piece, choice);
	}
	return text;
}

export function spotCount(pieces: Piece[]): number {
	return pieces.filter((piece) => piece.kind === "spot").length;
}

/** How many of the document's spots are still to choose. */
export function left(pieces: Piece[], choices: Choices): number {
	const total = spotCount(pieces);
	const chosen = choices
		.slice(0, total)
		.filter((choice) => choice !== null).length;
	return total - chosen;
}

/**
 * The next spot still to choose after `after`, going round to the start, or
 * -1 once every spot is chosen.
 */
export function nextOpen(
	choices: Choices,
	total: number,
	after: number,
): number {
	for (let step = 1; step <= total; step += 1) {
		const at = (after + step) % total;
		if ((choices[at] ?? null) === null) {
			return at;
		}
	}
	return -1;
}

/** What is left of a thing to settle, as the list beside the text says it. */
export function leftLabel(left: number, total: number): string {
	if (left === 0) {
		return "Settled";
	}
	return left === total
		? `${left.toLocaleString()} left`
		: `${left.toLocaleString()} of ${total.toLocaleString()} left`;
}

/** What is left of a document's spots, as its heading says it. */
export function spotsLeft(left: number, total: number): string {
	if (left === 0) {
		return total === 1 ? "The spot is settled" : "Every spot is settled";
	}
	const spots = total === 1 ? "spot" : "spots";
	return `${left.toLocaleString()} of ${total.toLocaleString()} ${spots} left`;
}

/**
 * What the merge screen opens the binder and details under, in place of a
 * document's id. Ids are uuids, so none can be this.
 */
export const BINDER = "binder";

/** Which version a binder card keeps. */
export type Side = "ours" | "theirs";

/** A side per binder card, by `cardKey`. A card with none is still open. */
export type Sides = Record<string, Side>;

/** The conflicts outside the text, one card each. */
export function cards(conflicts: Conflict[]): Conflict[] {
	return conflicts.filter((conflict) => conflict.kind !== "text");
}

/**
 * What a conflict is about, whatever each side has, which is how Rust finds
 * a choice again. A value is never part of it: a number can come back from
 * JSON as a float.
 */
export function cardKey(conflict: Conflict): string {
	const id = "id" in conflict ? (conflict.id ?? "") : "";
	const field = conflict.kind === "field" ? conflict.field : "";
	return `${conflict.kind} ${id} ${field}`;
}

export function cardsLeft(cards: Conflict[], sides: Sides): number {
	return cards.filter((card) => sides[cardKey(card)] === undefined).length;
}

/**
 * The conflicts to settle the other version's way, as `settle_binder` takes
 * them, or null while a card is still open. The rest keep this version's.
 */
export function theirSides(cards: Conflict[], sides: Sides): Conflict[] | null {
	if (cardsLeft(cards, sides) > 0) {
		return null;
	}
	return cards.filter((card) => sides[cardKey(card)] === "theirs");
}

/** Every node in the tree by id, with its name and the folder it sits in. */
export function named(
	nodes: TreeNode[],
	folder = "",
): Map<string, { name: string; folder: string }> {
	const found = new Map<string, { name: string; folder: string }>();
	for (const node of nodes) {
		const name = node.node === "folder" ? node.name : node.title;
		found.set(node.id, { name, folder });
		if (node.node === "folder") {
			for (const [id, inner] of named(node.children, name)) {
				found.set(id, inner);
			}
		}
	}
	return found;
}

// The fields Aurora names itself. A folder's own fields are named by the
// writer and show as typed.
const fieldNames: Record<string, string> = {
	title: "Title",
	subtitle: "Subtitle",
	author: "Author",
	contributors: "Contributors",
	series: "Series",
	seriesNumber: "Series number",
	language: "Language",
	blurb: "Blurb",
	publisher: "Publisher",
	publicationDate: "Publication date",
	isbn: "ISBN",
	keywords: "Keywords",
	copyright: "Copyright",
	exportFormats: "Export formats",
	identifier: "Identifier",
	kind: "Kind",
	target: "Target",
};

/** A "Changed on both" card's title: the field, then whose it is. */
export function fieldTitle(field: string, owner: string | null): string {
	const name = fieldNames[field] ?? field;
	return owner === null ? `${name} of the book` : `${name} of ${owner}`;
}

/** A field's value as a card shows it, empty when the side has none. */
export function fieldText(value: unknown): string {
	if (value === null || value === undefined) {
		return "";
	}
	if (typeof value === "object") {
		return Object.values(value)
			.map(fieldText)
			.filter((text) => text !== "")
			.join(", ");
	}
	return String(value);
}

/** The paragraphs of a stretch of text, blank lines being what parts them. */
export function paragraphs(text: string): string[] {
	return text
		.split(/\n[ \t]*\n/)
		.map((paragraph) => paragraph.trim())
		.filter((paragraph) => paragraph !== "");
}
