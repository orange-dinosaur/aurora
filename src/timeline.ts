// Which rows the timeline draws and which scenes sit in each. A row is a
// character or a place, and a scene sits in every row whose subject it names
// or tags, so one scene can show more than once down the same column.

import { appearancesIn, type Mentionable, type Subject } from "./lib/mentions";

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
