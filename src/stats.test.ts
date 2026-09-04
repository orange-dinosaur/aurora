import { describe, expect, test } from "vitest";
import { changed, start, wrote, IDLE } from "./sessions";
import type { Limit, Running } from "./sessions";
import { clock, onDay, running, sessionReading, today } from "./stats";
import type { PastSession } from "./types";

/** Noon on the fourth, where the writer is: a day is a local thing. */
const NOON = new Date(2026, 8, 4, 12).getTime();

/** A moment on the fourth of September, spelled the way the file spells it. */
function at(day: number, hour: number): string {
	return new Date(2026, 8, day, hour).toISOString();
}

function past(
	layer: "automatic" | "deliberate",
	began: string,
	documents: Record<string, { written: number; removed: number }>,
): PastSession {
	return {
		id: `id-${began}-${layer}`,
		layer,
		start: began,
		end: began,
		written: 0,
		removed: 0,
		net: 0,
		documents,
	};
}

/** A session with one change in it, the way the project view feeds them. */
function typed(document: string, words: number) {
	return wrote(IDLE, changed(document, NOON, words), 0).sessions;
}

describe("the sessions of one day", () => {
	const history = [
		past("automatic", at(3, 22), { scene: { written: 90, removed: 0 } }),
		past("automatic", at(4, 9), { scene: { written: 40, removed: 0 } }),
		past("deliberate", at(4, 10), { scene: { written: 60, removed: 0 } }),
	];

	test("yesterday's are left out", () => {
		expect(onDay(history, NOON).map((session) => session.layer)).toEqual([
			"automatic",
			"deliberate",
		]);
	});

	test("a day with nothing in it holds nothing", () => {
		expect(onDay(history, new Date(2026, 8, 6, 12).getTime())).toEqual([]);
	});
});

describe("what a document has gained", () => {
	test("the running sessions are counted apart", () => {
		const sessions = start(typed("scene", 120), NOON, 0).sessions;

		expect(running(sessions, "scene")).toEqual({
			automatic: 120,
			deliberate: 0,
		});
		expect(running(sessions, "elsewhere")).toEqual({
			automatic: 0,
			deliberate: 0,
		});
	});

	test("today is the file's sessions and the running ones together", () => {
		const history = [
			past("automatic", at(4, 9), {
				scene: { written: 40, removed: 0 },
			}),
			past("deliberate", at(4, 10), {
				scene: { written: 60, removed: 0 },
				notes: { written: 15, removed: 0 },
			}),
			past("automatic", at(3, 9), {
				scene: { written: 999, removed: 0 },
			}),
		];

		expect(today(history, typed("scene", 25), "scene", NOON)).toEqual({
			automatic: 65,
			deliberate: 60,
		});
		expect(today(history, IDLE, "notes", NOON)).toEqual({
			automatic: 0,
			deliberate: 15,
		});
	});

	test("a session that touched no document adds nothing", () => {
		const history = [past("automatic", at(4, 9), {})];

		expect(today(history, IDLE, "scene", NOON)).toEqual({
			automatic: 0,
			deliberate: 0,
		});
	});
});

describe("a span on the clock", () => {
	test("reads as minutes and seconds", () => {
		expect(clock(0)).toBe("0:00");
		expect(clock(9 * 1000)).toBe("0:09");
		expect(clock(4 * 60 * 1000 + 12 * 1000)).toBe("4:12");
	});

	test("takes an hour once it has run one", () => {
		expect(clock(3725 * 1000)).toBe("1:02:05");
	});

	// A deadline that has passed arrives here as a negative span, from the
	// second between the limit being met and the session closing.
	test("never runs backwards", () => {
		expect(clock(-5000)).toBe("0:00");
	});
});

describe("what a running session reads as", () => {
	/** The deliberate layer of a session just opened. */
	function opened(limit?: Limit): Running {
		const session = start(IDLE, NOON, 0, limit).sessions.deliberate;
		if (session === null) {
			throw new Error("starting opens a deliberate session");
		}
		return session;
	}

	test("without a limit, the time it has been open", () => {
		expect(sessionReading(opened(), NOON + 90 * 1000)).toBe("1:30");
	});

	test("a sprint on the clock, the time it has left", () => {
		const sprint = opened({ unit: "minutes", amount: 25 });
		expect(sessionReading(sprint, NOON + 60 * 1000)).toBe("24:00 left");
	});

	test("a sprint on words, its progress towards them", () => {
		const sprint = start(IDLE, NOON, 0, { unit: "words", amount: 500 });
		const session = wrote(
			sprint.sessions,
			changed("scene", NOON + 1000, 120),
			0,
		).sessions.deliberate;

		expect(session && sessionReading(session, NOON + 1000)).toBe(
			"120 of 500 words · 24%",
		);
	});
});
