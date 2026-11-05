import type { StructureLayout } from "../types";

// The same ranges `structure.rs` checks. The dialog keeps to them so Create is
// never refused, but Rust has the last word.
export const MAX_PARTS = 10;
export const MAX_CHAPTERS = 99;
export const MAX_SCENES = 20;

/** The number fields as typed, so one can be cleared and retyped. */
export type Draft = { parts: string; chapters: string; scenes: string };

export type Counts = { parts: number; chapters: number; scenes: number };

export function draftOf(layout: StructureLayout): Draft {
	return {
		parts: String(layout.parts),
		chapters: String(layout.chapters),
		scenes: String(layout.scenes),
	};
}

function whole(text: string): number | null {
	const trimmed = text.trim();
	return /^\d{1,3}$/.test(trimmed) ? Number(trimmed) : null;
}

/**
 * Reads the draft. Each field is its number when in range, or null. Only Your
 * own takes its parts from the draft; the others have a fixed count. Chapters
 * need at least one a part, so with no valid parts they are null too.
 */
export function read(
	draft: Draft,
	layout: StructureLayout,
): { parts: number | null; chapters: number | null; scenes: number | null } {
	const typed =
		layout.structure === "your-own" ? whole(draft.parts) : layout.parts;
	const parts =
		typed !== null && typed >= 1 && typed <= MAX_PARTS ? typed : null;
	const asked = whole(draft.chapters);
	const chapters =
		parts !== null &&
		asked !== null &&
		asked >= parts &&
		asked <= MAX_CHAPTERS
			? asked
			: null;
	const scenes = whole(draft.scenes);
	return {
		parts,
		chapters,
		scenes: scenes !== null && scenes <= MAX_SCENES ? scenes : null,
	};
}

/** The numbers a draft stands for, or null while any field is out of range. */
export function counts(draft: Draft, layout: StructureLayout): Counts | null {
	const { parts, chapters, scenes } = read(draft, layout);
	return parts !== null && chapters !== null && scenes !== null
		? { parts, chapters, scenes }
		: null;
}
