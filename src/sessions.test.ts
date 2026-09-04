import { describe, expect, test } from "vitest";
import {
	changed,
	IDLE,
	IDLE_GAP,
	start,
	stop,
	tick,
	wrote,
	type Change,
	type Sessions,
} from "./sessions";

const NOON = Date.UTC(2026, 8, 4, 12, 0, 0);

/** A moment, as minutes past noon. */
function at(minutes: number): number {
	return NOON + minutes * 60 * 1000;
}

function change(
	minutes: number,
	written: number,
	removed = 0,
	document = "scene",
): Change {
	return { at: at(minutes), document, written, removed };
}

/** Feeds a run of changes in order, keeping everything that closed. */
function run(changes: Change[], from: Sessions = IDLE) {
	let sessions = from;
	const closed = [];
	for (const one of changes) {
		const step = wrote(sessions, one);
		sessions = step.sessions;
		closed.push(...step.closed);
	}
	return { sessions, closed };
}

describe("the automatic layer", () => {
	test("the first change opens a session", () => {
		const { sessions, closed } = wrote(IDLE, change(0, 12));

		expect(closed).toEqual([]);
		expect(sessions.automatic).toMatchObject({
			layer: "automatic",
			start: at(0),
			last: at(0),
			written: 12,
			removed: 0,
		});
		expect(sessions.deliberate).toBeNull();
	});

	test("changes inside the gap belong to the same session", () => {
		const { sessions, closed } = run([
			change(0, 100),
			change(20, 50, 10),
			change(45, 30),
		]);

		expect(closed).toEqual([]);
		expect(sessions.automatic).toMatchObject({
			start: at(0),
			last: at(45),
			written: 180,
			removed: 10,
		});
	});

	test("a gap opens a new session and ends the old one where the typing stopped", () => {
		const { sessions, closed } = run([
			change(0, 100),
			change(10, 40),
			change(41, 25),
		]);

		expect(closed).toHaveLength(1);
		expect(closed[0]).toMatchObject({
			layer: "automatic",
			start: at(0),
			end: at(10),
			limitMet: false,
			written: 140,
		});
		expect(sessions.automatic).toMatchObject({
			start: at(41),
			written: 25,
		});
	});

	test("the clock alone closes a session that has gone quiet", () => {
		const typed = wrote(IDLE, change(0, 100)).sessions;

		expect(tick(typed, at(29)).closed).toEqual([]);
		expect(tick(typed, at(29)).sessions.automatic).not.toBeNull();

		const { sessions, closed } = tick(typed, at(0) + IDLE_GAP);
		expect(closed).toHaveLength(1);
		expect(closed[0]).toMatchObject({ end: at(0), written: 100 });
		expect(sessions.automatic).toBeNull();
	});

	test("what each document gained and lost is kept apart", () => {
		const { sessions } = run([
			change(0, 100, 0, "one"),
			change(5, 20, 45, "two"),
			change(9, 30, 5, "one"),
		]);

		expect(sessions.automatic?.documents.get("one")).toEqual({
			written: 130,
			removed: 5,
		});
		expect(sessions.automatic?.documents.get("two")).toEqual({
			written: 20,
			removed: 45,
		});
	});
});

describe("what a change reports", () => {
	test("a delta is the words it moved, not the words there are", () => {
		expect(changed("scene", at(0), 40)).toMatchObject({
			written: 40,
			removed: 0,
		});
		expect(changed("scene", at(0), -12)).toMatchObject({
			written: 0,
			removed: 12,
		});
	});

	test("typing a word then deleting it is one written and one removed", () => {
		const { sessions } = run([
			changed("scene", at(0), 1),
			changed("scene", at(1), -1),
		]);

		expect(sessions.automatic).toMatchObject({ written: 1, removed: 1 });
		expect(sessions.automatic?.documents.get("scene")).toEqual({
			written: 1,
			removed: 1,
		});
	});
});

describe("the deliberate layer", () => {
	test("a session spans a gap that closes the automatic one underneath", () => {
		const opened = start(
			wrote(IDLE, change(0, 40)).sessions,
			at(1),
		).sessions;
		const { sessions, closed } = run([change(60, 20)], opened);

		expect(closed).toHaveLength(1);
		expect(closed[0]?.layer).toBe("automatic");
		expect(sessions.deliberate).toMatchObject({
			start: at(1),
			written: 20,
			last: at(60),
		});
	});

	test("a change is counted on both layers", () => {
		const opened = start(IDLE, at(0)).sessions;
		const { sessions } = run([change(1, 60, 10)], opened);

		expect(sessions.automatic).toMatchObject({ written: 60, removed: 10 });
		expect(sessions.deliberate).toMatchObject({ written: 60, removed: 10 });
	});

	test("starting a session while one runs closes the first", () => {
		const opened = start(IDLE, at(0)).sessions;
		const { sessions, closed } = start(opened, at(30));

		expect(closed).toHaveLength(1);
		expect(closed[0]).toMatchObject({
			layer: "deliberate",
			start: at(0),
			end: at(30),
			limitMet: false,
		});
		expect(sessions.deliberate?.start).toBe(at(30));
	});
});

describe("sprints", () => {
	test("a word limit closes the sprint on the change that crosses it", () => {
		const opened = start(IDLE, at(0), {
			unit: "words",
			amount: 500,
		}).sessions;
		const { sessions, closed } = run(
			[change(1, 200), change(5, 250), change(9, 90)],
			opened,
		);

		expect(closed).toHaveLength(1);
		expect(closed[0]).toMatchObject({
			layer: "deliberate",
			end: at(9),
			limitMet: true,
			written: 540,
		});
		expect(sessions.deliberate).toBeNull();
		// The writing itself did not stop, so the layer underneath runs on.
		expect(sessions.automatic).toMatchObject({ written: 540 });
	});

	test("a time limit closes the sprint at its deadline, not when anyone looks", () => {
		const opened = start(IDLE, at(0), {
			unit: "minutes",
			amount: 15,
		}).sessions;
		const { sessions, closed } = tick(opened, at(20));

		expect(closed).toHaveLength(1);
		expect(closed[0]).toMatchObject({ end: at(15), limitMet: true });
		expect(sessions.deliberate).toBeNull();
	});

	test("a change after the deadline goes to the automatic layer alone", () => {
		const opened = start(IDLE, at(0), {
			unit: "minutes",
			amount: 15,
		}).sessions;
		const { sessions, closed } = run(
			[change(2, 80), change(18, 40)],
			opened,
		);

		expect(closed).toHaveLength(1);
		expect(closed[0]).toMatchObject({ end: at(15), written: 80 });
		expect(sessions.automatic).toMatchObject({ written: 120 });
	});

	test("stopping a sprint early leaves its limit unmet", () => {
		const opened = start(IDLE, at(0), {
			unit: "words",
			amount: 500,
		}).sessions;
		const { sessions, closed } = stop(
			run([change(1, 120)], opened).sessions,
			at(4),
		);

		expect(closed).toHaveLength(1);
		expect(closed[0]).toMatchObject({
			end: at(4),
			limitMet: false,
			written: 120,
			limit: { unit: "words", amount: 500 },
		});
		expect(sessions.deliberate).toBeNull();
		expect(sessions.automatic).not.toBeNull();
	});

	test("aiming at nothing is not a sprint", () => {
		const opened = start(IDLE, at(0), {
			unit: "words",
			amount: 0,
		}).sessions;
		const { sessions, closed } = run([change(1, 10)], opened);

		expect(closed).toEqual([]);
		expect(sessions.deliberate?.limit).toBeUndefined();
	});
});
