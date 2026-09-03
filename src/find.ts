// Finding text is done against a copy of the document rather than the document
// itself: the editor hands over its runs of text, this works out where the
// matches are, and the panel paints them. Nothing here touches a node, so a
// search can never dirty a document or land in the undo history.

/** A run of text as the editor holds it, and the block it sits in. */
export type Run = {
	/** The text node's key, which is how the panel finds it again. */
	key: string;
	text: string;
	/** The key of the paragraph or heading the run belongs to. */
	block: string;
};

/**
 * What Find is opened on when the writer clicks a hit in project-wide search.
 * A position in the file would not survive the trip — Find works in the
 * editor's nodes, and the search that found this one worked on a different
 * editor's — so the hit travels as the query and which occurrence of it this
 * is. Find counts the matches itself and steps to that one.
 */
export type Seed = {
	query: string;
	/** Counting from zero, in reading order, as `matches` returns them. */
	ordinal: number;
	/**
	 * How whatever counted the hits was reading. A mention was counted
	 * whole-word and case-sensitively, and Find has to read the same way or it
	 * would step to a different occurrence than the one that was clicked.
	 */
	reading?: Reading;
};

/** Where one match starts and ends, in the editor's own terms. */
export type Match = {
	fromKey: string;
	fromOffset: number;
	toKey: string;
	toOffset: number;
};

/**
 * How the query should be read. Both are off by default, which is what Find
 * and project-wide search want: a writer typing `rose` expects to be shown
 * `Rosemary` too.
 */
export type Reading = {
	/** Reject a match with a letter or digit against either edge. */
	wholeWord?: boolean;
	/** Tell `Rose` from `rose`. */
	caseSensitive?: boolean;
};

type Span = {
	key: string;
	/** Where this run begins in the joined text. */
	at: number;
	length: number;
};

const WORD = /[\p{L}\p{N}_]/u;

/** Whether nothing word-like sits against either edge of the match. */
function whole(text: string, at: number, length: number): boolean {
	return (
		!WORD.test(text[at - 1] ?? "") && !WORD.test(text[at + length] ?? "")
	);
}

/** The run holding `index`, and how far into it that is. */
function within(spans: Span[], index: number, end: boolean): Span | null {
	// A match's end sits on the far edge of the last run it covers, so an index
	// exactly on a boundary belongs to the run before it rather than after.
	return (
		spans.find((span) =>
			end
				? index > span.at && index <= span.at + span.length
				: index >= span.at && index < span.at + span.length,
		) ?? null
	);
}

/**
 * Every place `query` appears, in the order a writer would step through them.
 * Matching never crosses from one block into the next; whether it ignores case
 * and whether it stops at word edges are up to `reading`.
 */
export function matches(
	runs: Run[],
	query: string,
	reading: Reading = {},
): Match[] {
	if (query === "") {
		return [];
	}

	let text = "";
	const spans: Span[] = [];
	let block: string | null = null;

	for (const run of runs) {
		if (run.text === "") {
			continue;
		}
		// A newline between blocks, and none inside one: a query typed into a
		// field cannot hold a newline, so this is what stops a match running
		// from the end of one paragraph into the start of the next while
		// still letting one span a bold word in the middle of a sentence.
		if (block !== null && run.block !== block) {
			text += "\n";
		}
		block = run.block;
		spans.push({ key: run.key, at: text.length, length: run.text.length });
		text += run.text;
	}

	// Lowercasing is per character and a few of them grow when it happens —
	// Turkish İ becomes two — which would put every offset after it out by
	// one. When that happens the search is exact instead of being wrong.
	const folded = text.toLowerCase();
	const exact =
		reading.caseSensitive === true || folded.length !== text.length;
	const haystack = exact ? text : folded;
	const needle = exact ? query : query.toLowerCase();

	const found: Match[] = [];
	let at = haystack.indexOf(needle);

	while (at !== -1) {
		if (reading.wholeWord === true && !whole(haystack, at, needle.length)) {
			// A rejected place can still hold the start of a real match one
			// character along, so this steps rather than skipping the width.
			at = haystack.indexOf(needle, at + 1);
			continue;
		}

		const from = within(spans, at, false);
		const to = within(spans, at + needle.length, true);
		if (from !== null && to !== null) {
			found.push({
				fromKey: from.key,
				fromOffset: at - from.at,
				toKey: to.key,
				toOffset: at + needle.length - to.at,
			});
		}
		// One match cannot start inside another, the way a writer stepping
		// through them would count.
		at = haystack.indexOf(needle, at + needle.length);
	}

	return found;
}
