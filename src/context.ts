// The line of context under a search hit. A hit carries the whole block it
// sits in, which can be a paragraph of several hundred characters, and a
// results list wants one scannable line per hit. This cuts a window out of
// that block around the match. Pure text in, pure text out.

/** How much of the block is kept either side of the match. */
const PAD = 40;

/**
 * How far past the pad a cut may travel to land on a space. Beyond this the
 * text is one long unbroken word as far as the reader is concerned, and
 * cutting through it says more than dropping it would.
 */
const REACH = 12;

/** Shown at an end that was cut, and never at an end that was not. */
const ELLIPSIS = "…";

/** A block sliced into what leads up to the match, the match, and what follows. */
export type Context = {
	before: string;
	match: string;
	after: string;
};

/** The last space at or before `at`, within reach of it, or null. */
function spaceBefore(line: string, at: number): number | null {
	for (let i = at; i >= at - REACH && i >= 0; i -= 1) {
		if (/\s/.test(line[i])) {
			return i;
		}
	}

	return null;
}

/** The first space at or after `at`, within reach of it, or null. */
function spaceAfter(line: string, at: number): number | null {
	for (let i = at; i <= at + REACH && i < line.length; i += 1) {
		if (/\s/.test(line[i])) {
			return i;
		}
	}

	return null;
}

/**
 * The block around a match, cut to a line. `from` and `to` are the match's
 * offsets within `line`, as a `Hit` carries them. The match itself is never
 * cut, however long the query is: it is the one part the writer is looking at.
 */
export function contextOf(line: string, from: number, to: number): Context {
	// Cutting forward rather than back drops the partial word rather than
	// showing half of it.
	const opening = from - PAD;
	const space = opening > 0 ? spaceAfter(line, opening) : null;
	const start = opening <= 0 ? 0 : space === null ? opening : space + 1;

	// And cutting back at the far end does the same there.
	const closing = to + PAD;
	const end =
		closing >= line.length
			? line.length
			: (spaceBefore(line, closing) ?? closing);

	const before = line.slice(start, from);
	const after = line.slice(to, end);

	return {
		before: start > 0 ? ELLIPSIS + before.trimStart() : before,
		match: line.slice(from, to),
		after: end < line.length ? after.trimEnd() + ELLIPSIS : after,
	};
}
