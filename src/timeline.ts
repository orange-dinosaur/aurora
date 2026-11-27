// Which rows the timeline draws and which scenes sit in each. A row is a
// character or a place, and a scene sits in every row whose subject it names
// or tags, so one scene can show more than once down the same column.

import type { Place } from "./bindings/Place";
import { appearancesIn, type Mentionable, type Subject } from "./lib/mentions";

/**
 * Where a dragged scene would go: the gap it lands in, counted from the gap
 * before the first scene, the place `place_scene` stores, and the scene on its
 * right, if any.
 */
export type Drop = { gap: number; place: Place; before: string | null };

/**
 * The scene and every scene placed after it, directly or down a chain. They
 * travel together, so a drop after any of them would make a loop.
 */
export function chainOf(
	scene: string,
	placed: Partial<Record<string, Place>>,
): Set<string> {
	const chain = new Set([scene]);
	let grew = true;
	while (grew) {
		grew = false;
		for (const [id, place] of Object.entries(placed)) {
			if (
				typeof place === "object" &&
				chain.has(place.after) &&
				!chain.has(id)
			) {
				chain.add(id);
				grew = true;
			}
		}
	}
	return chain;
}

/**
 * Where `moving` lands when let go `x` pixels right of the first scene's left
 * edge, with columns `pitch` apart. The nearest gap wins. Null for a gap that
 * leaves the scene where it is or falls inside its own chain.
 */
export function dropAt(
	x: number,
	pitch: number,
	order: string[],
	moving: string,
	placed: Partial<Record<string, Place>>,
): Drop | null {
	const gap = Math.min(order.length, Math.max(0, Math.round(x / pitch)));
	const left = gap === 0 ? null : order[gap - 1];
	if (
		order[gap] === moving ||
		(left !== null && chainOf(moving, placed).has(left))
	) {
		return null;
	}
	return {
		gap,
		place: left === null ? "first" : { after: left },
		before: order[gap] ?? null,
	};
}

/** One row: a subject, or `null` for the scenes that name none. */
export type TimelineRow = {
	subject: Subject | null;
	/** The ids of the scenes that sit in this row. */
	scenes: Set<string>;
};

/**
 * The rows for `section`'s subjects ("Characters" or "Locations"), each in the
 * order its subject is first met in story order, then one row for the scenes
 * that name none of them. A subject no scene names has no row, and the last
 * row is left out when every scene names someone.
 *
 * A `hidden` subject has no row either, but a scene that names only hidden
 * subjects still names someone, so it is not moved to the last row.
 */
export function rowsOf(
	order: string[],
	documents: Mentionable[],
	section: string,
	screenplay: boolean,
	hidden: ReadonlySet<string> = new Set(),
): TimelineRow[] {
	const subjects = subjectsOf(documents, section);
	const byId = new Map(documents.map((document) => [document.id, document]));
	const rows = new Map<string, TimelineRow>();
	const nobody = new Set<string>();

	for (const scene of order) {
		const document = byId.get(scene);
		const named =
			document === undefined
				? []
				: appearancesIn(document, subjects, screenplay);
		if (named.length === 0) {
			nobody.add(scene);
		}
		for (const { subject } of named) {
			const row = rows.get(subject.id) ?? { subject, scenes: new Set() };
			row.scenes.add(scene);
			rows.set(subject.id, row);
		}
	}

	const all = [...rows.values()].filter(
		(row) => row.subject === null || !hidden.has(row.subject.id),
	);
	if (nobody.size > 0) {
		all.push({ subject: null, scenes: nobody });
	}
	return all;
}

/** The pages filed under `section`, by title, which is what the row list offers. */
export function subjectsOf(
	documents: Mentionable[],
	section: string,
): Subject[] {
	return documents
		.filter((document) => document.trail[0] === section)
		.map(({ id, title, names }) => ({ id, title, names }))
		.sort((one, other) => one.title.localeCompare(other.title));
}
