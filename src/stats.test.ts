import { describe, expect, test } from "vitest";
import { changed, start, wrote, IDLE } from "./sessions";
import type { Limit, Running } from "./sessions";
import {
	clock,
	dayTally,
	onDay,
	running,
	sessionReading,
	today,
	unexplained,
} from "./stats";
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

describe("the project's day, across the three numbers", () => {
	/** A closed session with the three numbers the file holds for it. */
	function whole(
		layer: "automatic" | "deliberate",
		began: string,
		numbers: { written: number; removed: number; net: number },
	): PastSession {
		return { ...past(layer, began, {}), ...numbers };
	}

	test("the file's sessions are summed inside their own layer", () => {
		const history = [
			whole("automatic", at(4, 9), {
				written: 400,
				removed: 40,
				net: 360,
			}),
			whole("automatic", at(4, 11), {
				written: 200,
				removed: 0,
				net: 200,
			}),
			whole("deliberate", at(4, 10), {
				written: 300,
				removed: 20,
				net: 280,
			}),
			whole("automatic", at(3, 9), {
				written: 999,
				removed: 0,
				net: 999,
			}),
		];

		expect(dayTally(history, IDLE, 0, NOON)).toEqual({
			automatic: { written: 600, removed: 40, net: 560 },
			deliberate: { written: 300, removed: 20, net: 280 },
		});
	});

	// A running session has no net of its own yet: it is the project's count
	// now less the count the session opened on.
	test("a running session's net comes from the count now", () => {
		const open = start(IDLE, NOON, 1000);
		const typed = wrote(open.sessions, changed("scene", NOON, 50), 1000);

		expect(dayTally([], typed.sessions, 1050, NOON).deliberate).toEqual({
			written: 50,
			removed: 0,
			net: 50,
		});
	});

	test("nothing today is three noughts", () => {
		expect(dayTally([], IDLE, 4000, NOON)).toEqual({
			automatic: { written: 0, removed: 0, net: 0 },
			deliberate: { written: 0, removed: 0, net: 0 },
		});
	});
});

describe("what net does not say", () => {
	test("nothing, when every word went through the editor", () => {
		expect(unexplained({ written: 400, removed: 40, net: 360 })).toBe(0);
	});

	// The case the panel exists for: a document deleted whole moves the
	// project's count and passes neither of the other two.
	test("the words a deleted document took with it", () => {
		expect(unexplained({ written: 400, removed: 40, net: -600 })).toBe(
			-960,
		);
	});

	test("and words that arrived from outside", () => {
		expect(unexplained({ written: 10, removed: 0, net: 2010 })).toBe(2000);
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
