// Searching the whole project, over runs that have already been read out of
// each document. Nothing here knows about Lexical, Rust or React: it takes
// text and gives back the hits, so it can be tested on its own.

import { matches, type Match, type Run } from "./find";

/** A document as search reads it, once its runs have been taken. */
export type Searchable = {
	id: string;
	title: string;
	/** The folders it sits in, as the row labels it. */
	trail: string[];
	/** null when the document's file could not be read. */
	runs: Run[] | null;
};

/**
 * One occurrence of the query in a document's prose, carrying the block it
 * sits in so the row can show it in context. Whether that context is the whole
 * block or a window cut out of it is the panel's business, not this module's.
 */
export type Hit = {
	/**
	 * Which occurrence this is within its document, counting from zero. This
	 * is the number Find is seeded with, so it counts prose hits only — a
	 * title hit is not an occurrence in the text and has no position to step
	 * to.
	 */
	ordinal: number;
	/** The text of the block the match sits in. */
	line: string;
	/** Where the match starts and ends within `line`. */
	from: number;
	to: number;
};

/** Everything found in one document, and what it is called. */
export type Group = {
	id: string;
	title: string;
	trail: string[];
	/** Whether the query is in the document's name. */
	titleHit: boolean;
	hits: Hit[];
	/**
	 * How many rows this group has: its prose hits plus its title hit. The
	 * panel caps how many it draws and this stays the true number, so a
	 * heading never disagrees with what expanding it would show.
	 */
	count: number;
};

/** A document search could not look inside. */
export type Unreadable = {
	id: string;
	title: string;
	trail: string[];
};

export type Results = {
	/** Only documents with something in them, in the order they were given. */
	groups: Group[];
	unreadable: Unreadable[];
};

/**
 * Below this a query matches so much of a novel that the answer is no use, and
 * the first keystroke of every search would sweep the project for nothing.
 */
export const MIN_QUERY = 2;

const NOTHING: Results = { groups: [], unreadable: [] };

/** One of the document's blocks, filled in as its runs are walked. */
type Block = { text: string };

/** A run's block, and where the run begins within it. */
export type Located = { block: Block; start: number };

/**
 * Every run placed in its block, with the block's runs joined the way
 * `matches` joins them. Empty runs are skipped here for the same reason they
 * are skipped there: so the offsets the two work out agree.
 */
export function locate(runs: Run[]): Map<string, Located> {
	const located = new Map<string, Located>();
	let block: Block | null = null;
	let previous: string | null = null;

	for (const run of runs) {
		if (run.text === "") {
			continue;
		}
		if (block === null || run.block !== previous) {
			block = { text: "" };
			previous = run.block;
		}

		located.set(run.key, { block, start: block.text.length });
		block.text += run.text;
	}

	return located;
}

/** A match as a panel draws it: the line it sits in, and where in that line. */
export type Line = { line: string; from: number; to: number };

/**
 * The line a match sits in, and where in that line it starts and ends. Null
 * when the match came from runs other than the ones that were located, which
 * is the only way its keys can be unknown here.
 */
export function lineOf(
	located: Map<string, Located>,
	match: Match,
): Line | null {
	// A match never crosses a block boundary, so where it starts and where it
	// ends are in the same block.
	const from = located.get(match.fromKey);
	const to = located.get(match.toKey);
	if (from === undefined || to === undefined) {
		return null;
	}

	return {
		line: from.block.text,
		from: from.start + match.fromOffset,
		to: to.start + match.toOffset,
	};
}

/** The hits in one document's prose, in reading order. */
function hitsIn(runs: Run[], query: string): Hit[] {
	const located = locate(runs);
	const hits: Hit[] = [];

	for (const match of matches(runs, query)) {
		const where = lineOf(located, match);
		if (where !== null) {
			hits.push({ ordinal: hits.length, ...where });
		}
	}

	return hits;
}

/** Everything in the project that the query is in. */
export function search(documents: Searchable[], query: string): Results {
	if (query.length < MIN_QUERY) {
		return NOTHING;
	}

	const needle = query.toLowerCase();
	const groups: Group[] = [];
	const unreadable: Unreadable[] = [];

	for (const { id, title, trail, runs } of documents) {
		if (runs === null) {
			unreadable.push({ id, title, trail });
			continue;
		}

		const titleHit = title.toLowerCase().includes(needle);
		const hits = hitsIn(runs, query);
		if (!titleHit && hits.length === 0) {
			continue;
		}

		groups.push({
			id,
			title,
			trail,
			titleHit,
			hits,
			count: hits.length + (titleHit ? 1 : 0),
		});
	}

	return { groups, unreadable };
}
