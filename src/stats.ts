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
