/**
 * The two layers of a writing session, as pure logic.
 *
 * Underneath, an automatic session is detected as the writer works: it opens on
 * the first change after a gap of silence and closes when the same gap passes
 * again. Over it sits at most one deliberate session, opened by hand, which
 * spans any gap. A deliberate session given a limit is a sprint, and closes
 * itself when the limit is met.
 *
 * The two layers are counted separately and on purpose, so every number the app
 * shows can say which layer it came from. A change feeds both.
 *
 * Nothing here knows about timers, React or the disk. Every function is handed
 * the moment it is acting at, so a test can hand it any clock it likes.
 */

import type { SessionRecord } from "./types";

/** How long a silence has to run before it closes an automatic session. */
export const IDLE_GAP = 30 * 60 * 1000;

export type Layer = "automatic" | "deliberate";

/**
 * What a sprint is aiming at. The shape matches the record Rust writes, so a
 * closed sprint can go to disk as it stands.
 */
export type Limit =
	{ unit: "minutes"; amount: number } | { unit: "words"; amount: number };

/** What a session has added and removed. */
export interface Counts {
	written: number;
	removed: number;
}

/** One change the editor saw: what it added and removed, and where. */
export interface Change extends Counts {
	at: number;
	/** The id of the document that changed. */
	document: string;
}

/**
 * One change as the editor reports it, from the difference in the document's
 * word count. Typing a word and deleting it again is one written and one
 * removed, rather than two written or nothing at all; a change that both adds
 * and removes within itself shows only its balance, since a word count is all
 * the editor has to go on.
 */
export function changed(document: string, at: number, delta: number): Change {
	return {
		at,
		document,
		written: Math.max(delta, 0),
		removed: Math.max(-delta, 0),
	};
}

/** A session that is still running. */
export interface Running extends Counts {
	layer: Layer;
	start: number;
	/** The last change this session saw, which is where it ends if it goes quiet. */
	last: number;
	limit?: Limit;
	/** What the project held when this opened, which is where net starts from. */
	words: number;
	documents: Map<string, Counts>;
}

/** A session that has closed, with everything a record of it needs. */
export interface Closed extends Counts {
	layer: Layer;
	start: number;
	end: number;
	limit?: Limit;
	limitMet: boolean;
	/**
	 * The project's word count at the end less its count at the start. It can
	 * disagree with written less removed, and is meant to: a document deleted
	 * whole shows up here and in neither of the other two.
	 */
	net: number;
	documents: Map<string, Counts>;
}

/** Both layers as they stand. */
export interface Sessions {
	automatic: Running | null;
	deliberate: Running | null;
}

/** Nothing running: what the app holds until the writer types or asks. */
export const IDLE: Sessions = { automatic: null, deliberate: null };

/** Where the layers are now, and whatever closed on the way there. */
export interface Step {
	sessions: Sessions;
	/** Closed in the order they closed, ready to record once net is known. */
	closed: Closed[];
}

function open(layer: Layer, at: number, words: number, limit?: Limit): Running {
	return {
		layer,
		start: at,
		last: at,
		// Aiming at nothing is the same as not aiming, which keeps a sprint
		// that could never end out of the model.
		limit: limit && limit.amount > 0 ? limit : undefined,
		words,
		written: 0,
		removed: 0,
		documents: new Map(),
	};
}

function shut(
	running: Running,
	end: number,
	limitMet: boolean,
	words: number,
): Closed {
	return {
		layer: running.layer,
		start: running.start,
		end,
		limit: running.limit,
		limitMet,
		net: words - running.words,
		written: running.written,
		removed: running.removed,
		documents: running.documents,
	};
}

/** The same session with one more change in it. Nothing is written in place. */
function fed(running: Running, change: Change): Running {
	const documents = new Map(running.documents);
	const was = documents.get(change.document) ?? { written: 0, removed: 0 };
	documents.set(change.document, {
		written: was.written + change.written,
		removed: was.removed + change.removed,
	});

	return {
		...running,
		last: change.at,
		written: running.written + change.written,
		removed: running.removed + change.removed,
		documents,
	};
}

/** When a sprint on the clock closes itself, or null if it is not one. */
function deadline(running: Running): number | null {
	return running.limit?.unit === "minutes"
		? running.start + running.limit.amount * 60 * 1000
		: null;
}

function wordsReached(running: Running): boolean {
	return (
		running.limit?.unit === "words" &&
		running.written >= running.limit.amount
	);
}

/**
 * Everything that closes by the passing of time alone, at `at`, with `words`
 * saying what the project holds. Both the idle gap and a sprint's deadline are
 * read this way, so the same rules apply whether the moment arrived with a
 * change or with the clock.
 *
 * A session closing here ended in the past, and the count it is measured
 * against is the one there is now. Nothing wrote in between, which is why the
 * session ended, so the two are the same count in every case but a file changed
 * outside Aurora.
 */
export function tick(sessions: Sessions, at: number, words: number): Step {
	let { automatic, deliberate } = sessions;
	const closed: Closed[] = [];

	// A sprint ends at its deadline, not whenever anyone next looked, so a
	// change arriving after it belongs to the automatic layer alone.
	if (deliberate) {
		const due = deadline(deliberate);
		if (due !== null && at >= due) {
			closed.push(shut(deliberate, due, true, words));
			deliberate = null;
		}
	}

	// A silence closes the automatic session where the typing stopped, not
	// where it started again.
	if (automatic && at - automatic.last >= IDLE_GAP) {
		closed.push(shut(automatic, automatic.last, false, words));
		automatic = null;
	}

	return { sessions: { automatic, deliberate }, closed };
}

/**
 * A change the editor saw, fed to whichever layers are running. `words` is what
 * the project held before it: a session opening on this change counts the
 * change itself, so its net has to start from the count the change moved.
 */
export function wrote(sessions: Sessions, change: Change, words: number): Step {
	const { sessions: now, closed } = tick(sessions, change.at, words);
	const automatic = fed(
		now.automatic ?? open("automatic", change.at, words),
		change,
	);
	let deliberate = now.deliberate;

	if (deliberate) {
		deliberate = fed(deliberate, change);
		// A change cannot be split, so the one that crosses the line counts in
		// full and the sprint closes with it.
		if (wordsReached(deliberate)) {
			const after = words + change.written - change.removed;
			closed.push(shut(deliberate, change.at, true, after));
			deliberate = null;
		}
	}

	return { sessions: { automatic, deliberate }, closed };
}

/**
 * Start session, with a limit if the writer set one. Only one deliberate
 * session runs at a time, so an earlier one is closed here rather than being
 * left to whatever offered the button.
 */
export function start(
	sessions: Sessions,
	at: number,
	words: number,
	limit?: Limit,
): Step {
	const { sessions: now, closed } = tick(sessions, at, words);
	if (now.deliberate) {
		closed.push(shut(now.deliberate, at, false, words));
	}

	return {
		sessions: {
			automatic: now.automatic,
			deliberate: open("deliberate", at, words, limit),
		},
		closed,
	};
}

/**
 * Both layers closed where they stand, for a writer who is leaving. The
 * automatic session ends at its last change rather than now, since nothing was
 * written between the two; the deliberate one ends here, because the writer
 * held it open until here.
 */
export function ended(sessions: Sessions, at: number, words: number): Step {
	const { sessions: now, closed } = tick(sessions, at, words);
	if (now.automatic) {
		closed.push(shut(now.automatic, now.automatic.last, false, words));
	}
	if (now.deliberate) {
		closed.push(shut(now.deliberate, at, false, words));
	}

	return { sessions: IDLE, closed };
}

/**
 * A closed session as the history file takes it: times as RFC 3339, the tally
 * as a plain object, and no id, which is Rust's to mint.
 */
export function recorded(session: Closed): SessionRecord {
	return {
		layer: session.layer,
		start: new Date(session.start).toISOString(),
		end: new Date(session.end).toISOString(),
		...(session.limit === undefined ? {} : { limit: session.limit }),
		limitMet: session.limitMet,
		written: session.written,
		removed: session.removed,
		net: session.net,
		documents: Object.fromEntries(session.documents),
	};
}

/**
 * Stop session. A sprint stopped by hand did not meet its limit. The automatic
 * layer underneath runs on: the writer ended a session, not the writing.
 */
export function stop(sessions: Sessions, at: number, words: number): Step {
	const { sessions: now, closed } = tick(sessions, at, words);
	if (now.deliberate) {
		closed.push(shut(now.deliberate, at, false, words));
	}

	return { sessions: { automatic: now.automatic, deliberate: null }, closed };
}
