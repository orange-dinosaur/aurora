// The merge screen's arithmetic: what a document reads as once its spots are
// chosen, and how much is left to choose.

import type { Piece } from "../types";

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

/** The paragraphs of a stretch of text, blank lines being what parts them. */
export function paragraphs(text: string): string[] {
	return text
		.split(/\n[ \t]*\n/)
		.map((paragraph) => paragraph.trim())
		.filter((paragraph) => paragraph !== "");
}
