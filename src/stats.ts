// What a session put into one document, gathered from the two places the
// numbers live: the sessions running now, which are only in memory, and the
// ones the history file already holds.

import type { Sessions } from "./sessions";
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
