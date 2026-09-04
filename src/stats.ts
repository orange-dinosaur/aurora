// What a session put into one document, gathered from the two places the
// numbers live: the sessions running now, which are only in memory, and the
// ones the history file already holds.

import type { Running, Sessions } from "./sessions";
// The words half of a sprint is the same claim as a word target, so it is
// worded by the same function rather than by a second one that drifts from it.
import { reading } from "./Target";
import type { PastSession } from "./types";

/**
 * A number for each layer. They are kept apart everywhere rather than added
 * up, because a session the writer opened and one detected underneath them are
 * not the same claim about the work.
 */
export type Totals = { automatic: number; deliberate: number };

/** What the sessions running now have put into one document. */
export function running(sessions: Sessions, document: string): Totals {
	return {
		automatic: sessions.automatic?.documents.get(document)?.written ?? 0,
		deliberate: sessions.deliberate?.documents.get(document)?.written ?? 0,
	};
}

/** A span as a clock reads it: `4:12`, and `1:02:05` once it passes the hour. */
export function clock(ms: number): string {
	const seconds = Math.max(Math.round(ms / 1000), 0);
	const rest = String(seconds % 60).padStart(2, "0");
	const minutes = Math.floor(seconds / 60) % 60;
	const hours = Math.floor(seconds / 3600);

	return hours === 0
		? `${minutes}:${rest}`
		: `${hours}:${String(minutes).padStart(2, "0")}:${rest}`;
}

/**
 * What a running deliberate session says about itself in one line. A sprint
 * reads as its progress towards the limit it was given; a session without one
 * has only the time it has been open to report.
 */
export function sessionReading(session: Running, at: number): string {
	if (session.limit?.unit === "words") {
		return reading(session.written, session.limit.amount);
	}

	if (session.limit?.unit === "minutes") {
		const due = session.start + session.limit.amount * 60 * 1000;
		return `${clock(due - at)} left`;
	}

	return clock(at - session.start);
}

/**
 * The sessions that began on the same day as `at`, by the writer's own clock
 * rather than UTC: a day is where they live, not where the file is written.
 */
export function onDay(history: PastSession[], at: number): PastSession[] {
	const day = new Date(at).toDateString();
	return history.filter(
		(session) => new Date(session.start).toDateString() === day,
	);
}

/**
 * The history file in the order a reader wants it, latest first. The file is
 * appended to as sessions close, so it arrives the other way round, and it is
 * sorted rather than reversed in case anything ever writes to it out of order.
 */
export function newest(history: PastSession[]): PastSession[] {
	return [...history].sort(
		(one, other) =>
			new Date(other.start).getTime() - new Date(one.start).getTime(),
	);
}

/** A session as one document saw it: the record, and what went in here. */
export interface Visit {
	session: PastSession;
	written: number;
	removed: number;
}

/**
 * The sessions that touched one document, latest first. A session that never
 * reached it is left out rather than listed with noughts, so the list answers
 * "when was this written" and not "what else was I doing".
 *
 * There is no net here, and there cannot be: net is the whole project's count
 * moving, and no single document owns a share of it.
 */
export function visits(history: PastSession[], document: string): Visit[] {
	return newest(history).flatMap((session) => {
		const counts = session.documents?.[document];
		return counts === undefined ? [] : [{ session, ...counts }];
	});
}

/** How long a session ran, from the two moments the file holds. */
export function lasted(session: PastSession): string {
	return clock(
		new Date(session.end).getTime() - new Date(session.start).getTime(),
	);
}

/**
 * What a session was, in the vocabulary the decision uses: which layer it came
 * from, and what it was aiming at when it had something to aim at. A sprint is
 * a deliberate session with a limit, not a third kind of thing, so it is
 * spelled out here rather than stored as one.
 */
export function sessionKind(session: PastSession): string {
	if (session.layer === "automatic") {
		return "Automatic";
	}

	if (session.limit === undefined) {
		return "Session";
	}

	const aim = `Sprint · ${session.limit.amount.toLocaleString()} ${session.limit.unit}`;
	return session.limitMet === true ? `${aim} · met` : `${aim} · stopped`;
}

/**
 * A session's three project-wide numbers. Written and removed are counted where
 * the editor sees them; net is the project's own word count moving. They are
 * kept apart because they can disagree and both are true.
 */
export interface Tally {
	written: number;
	removed: number;
	net: number;
}

const NOTHING: Tally = { written: 0, removed: 0, net: 0 };

function added(one: Tally, other: Tally): Tally {
	return {
		written: one.written + other.written,
		removed: one.removed + other.removed,
		net: one.net + other.net,
	};
}

/**
 * What a session still running has done to the project. Its net is worked out
 * here rather than held: it is the count now less the count the session opened
 * on, and it only settles when the session closes.
 */
function sofar(session: Running | null, words: number): Tally {
	return session === null
		? NOTHING
		: {
				written: session.written,
				removed: session.removed,
				net: words - session.words,
			};
}

/**
 * The whole project's three numbers for the day `at` falls in, per layer: what
 * the file holds from the sessions that began today, plus whatever is still
 * running.
 */
export function dayTally(
	history: PastSession[],
	sessions: Sessions,
	words: number,
	at: number,
): { automatic: Tally; deliberate: Tally } {
	return onDay(history, at).reduce(
		(all, session) => {
			const tally = {
				written: session.written,
				removed: session.removed,
				net: session.net,
			};

			return session.layer === "automatic"
				? { ...all, automatic: added(all.automatic, tally) }
				: { ...all, deliberate: added(all.deliberate, tally) };
		},
		{
			automatic: sofar(sessions.automatic, words),
			deliberate: sofar(sessions.deliberate, words),
		},
	);
}

/**
 * How far net is from what written and removed would predict. It is nought
 * whenever every word that moved went through the editor, and something else
 * whenever words arrived or left another way: a document deleted whole, a file
 * changed outside Aurora, a chapter pasted in from elsewhere.
 */
export function unexplained(tally: Tally): number {
	return tally.net - (tally.written - tally.removed);
}

/**
 * What has gone into one document today: what the file holds from the sessions
 * that began today, plus what the running ones hold, since nothing is written
 * down until a session closes.
 */
export function today(
	history: PastSession[],
	sessions: Sessions,
	document: string,
	at: number,
): Totals {
	return onDay(history, at).reduce(
		(all, session) => {
			const written = session.documents?.[document]?.written ?? 0;
			return session.layer === "automatic"
				? { ...all, automatic: all.automatic + written }
				: { ...all, deliberate: all.deliberate + written };
		},
		running(sessions, document),
	);
}
