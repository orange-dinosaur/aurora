// Which rows the timeline draws and which scenes sit in each. A row is a
// character or a place, and a scene sits in every row whose subject it names
// or tags, so one scene can show more than once down the same column.

import type { Marker } from "./bindings/Marker";
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
 * The gap nearest `x` pixels right of the first scene's left edge, with
 * columns `pitch` apart: 0 before the first scene, `count` after the last.
 */
export function gapAt(x: number, pitch: number, count: number): number {
	return Math.min(count, Math.max(0, Math.round(x / pitch)));
}

/** A marker and the columns it names, `from` its scene up to `to`. */
export type Stretch = { marker: Marker; from: number; to: number };

/**
 * Each marker whose scene is in `order`, left to right, running up to the
 * next marker or the last scene.
 */
export function stretchesOf(markers: Marker[], order: string[]): Stretch[] {
	const at = new Map(order.map((id, index) => [id, index]));
	const starts = markers
		.flatMap((marker) => {
			const from = at.get(marker.before);
			return from === undefined ? [] : [{ marker, from }];
		})
		.sort((one, other) => one.from - other.from);
	return starts.map((start) => ({
		...start,
		to: starts.find((next) => next.from > start.from)?.from ?? order.length,
	}));
}

/**
 * The scene a marker put in `gap` would sit before, or null past the last
 * scene or where a marker already starts, the dragged one's own gap included.
 */
export function markerAt(
	gap: number,
	order: string[],
	stretches: Stretch[],
): string | null {
	return gap >= order.length ||
		stretches.some((stretch) => stretch.from === gap)
		? null
		: order[gap];
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
	return dropIn(gapAt(x, pitch, order.length), order, moving, placed);
}

/**
 * Where `moving` lands one step earlier (-1) or later (1) without a drag: the
 * nearest gap that way a drop would accept, so a scene steps over its own
 * followers. Null at either end.
 */
export function stepped(
	order: string[],
	moving: string,
	placed: Partial<Record<string, Place>>,
	step: -1 | 1,
): Drop | null {
	const at = order.indexOf(moving);
	if (at === -1) {
		return null;
	}
	for (
		let gap = step < 0 ? at - 1 : at + 2;
		gap >= 0 && gap <= order.length;
		gap += step
	) {
		const drop = dropIn(gap, order, moving, placed);
		if (drop !== null) {
			return drop;
		}
	}
	return null;
}

/**
 * The scene a marker would start before one scene earlier (-1) or later (1),
 * or null where that is past an end or another marker starts.
 */
export function markerStepped(
	stretch: Stretch,
	step: -1 | 1,
	order: string[],
	stretches: Stretch[],
): string | null {
	const gap = stretch.from + step;
	return gap < 0 ? null : markerAt(gap, order, stretches);
}

/** Which of the five block colours the row or marker at `index` takes, 1 to 5. */
export function hue(index: number): number {
	return (index % 5) + 1;
}

function dropIn(
	gap: number,
	order: string[],
	moving: string,
	placed: Partial<Record<string, Place>>,
): Drop | null {
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
