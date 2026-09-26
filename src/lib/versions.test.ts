import { describe, expect, it } from "vitest";
import { timeOfDay, when } from "../dates";
import type { Change, Version } from "../types";
import { byDay, counts, detail, duration, label, summary } from "./versions";

function version(at: Date, rest: Partial<Version> = {}): Version {
	return {
		id: at.toISOString(),
		at: at.toISOString(),
		kind: "session",
		name: null,
		minutes: null,
		written: null,
		removed: null,
		author: "Aurora",
		words: null,
		...rest,
	};
}

describe("byDay", () => {
	const now = new Date(2026, 8, 23, 18, 0);

	it("heads today and yesterday by name and older days by date", () => {
		const older = new Date(2026, 8, 12, 9, 0);
		const list = [
			version(new Date(2026, 8, 23, 15, 40)),
			version(new Date(2026, 8, 23, 9, 15)),
			version(new Date(2026, 8, 22, 20, 0)),
			version(older),
		];

		const days = byDay(list, now);

		expect(days.map((day) => day.heading)).toEqual([
			"Today",
			"Yesterday",
			when(older.toISOString()),
		]);
		expect(days.map((day) => day.versions.length)).toEqual([2, 1, 1]);
	});

	it("gives an empty list no days", () => {
		expect(byDay([], now)).toEqual([]);
	});
});

describe("duration", () => {
	it("counts minutes under an hour", () => {
		expect(duration(42)).toBe("42 min");
	});

	it("gives hours and padded minutes from an hour", () => {
		expect(duration(64)).toBe("1 h 04");
	});
});

describe("counts", () => {
	it("leaves out whichever count is nought", () => {
		expect(counts(610, 0)).toBe("+610");
		expect(counts(0, 80)).toBe("−80");
		expect(counts(240, 80)).toBe("+240 −80");
		expect(counts(0, 0)).toBe("");
	});
});

describe("detail", () => {
	const at = new Date(2026, 8, 23, 15, 40);
	const time = timeOfDay(at.toISOString());

	it("describes a session by its length and its words", () => {
		const session = version(at, { minutes: 42, written: 610, removed: 0 });
		expect(detail(session)).toBe(`${time} · session, 42 min · +610`);
	});

	it("describes leaving the project", () => {
		const closing = version(at, {
			kind: "closing",
			written: 0,
			removed: 0,
		});
		expect(detail(closing)).toBe(`${time} · on leaving`);
	});

	it("gives a named version its words when it has them", () => {
		const named = version(at, {
			kind: "named",
			name: "Before the mill argument",
			words: 1148,
		});
		expect(detail(named)).toBe(
			`${time} · ${(1148).toLocaleString()} words · you`,
		);
		expect(detail({ ...named, words: null })).toBe(`${time} · you`);
	});

	it("credits a commit made outside Aurora to its author", () => {
		const foreign = version(at, { kind: null, author: "Ines" });
		expect(detail(foreign)).toBe(`${time} · Ines`);
	});
});

describe("summary", () => {
	function change(rest: Partial<Change>): Change {
		return {
			id: "id",
			folder: false,
			before: "Manuscript/Chapter 1/Scene.md",
			after: "Manuscript/Chapter 1/Scene.md",
			renamed: false,
			moved: false,
			edited: false,
			reordered: [],
			...rest,
		};
	}

	it("names a new or deleted document or folder by where it is", () => {
		expect(summary(change({ before: null }))).toEqual({
			name: "Scene",
			what: "New document",
		});
		expect(
			summary(
				change({
					folder: true,
					after: null,
					before: "Manuscript/Part",
				}),
			),
		).toEqual({ name: "Part", what: "Deleted folder" });
	});

	it("lists every change to one document in one line", () => {
		expect(
			summary(
				change({
					before: "Manuscript/Chapter 1/Old.md",
					after: "Manuscript/Chapter 3/Scene.md",
					renamed: true,
					moved: true,
					edited: true,
				}),
			),
		).toEqual({
			name: "Scene",
			what: "Renamed from “Old”, moved to Chapter 3, edited",
		});
	});

	it("says where each document moved within a folder", () => {
		expect(
			summary(
				change({
					folder: true,
					before: "Manuscript/Chapter 1",
					after: "Manuscript/Chapter 1",
					reordered: [
						{ name: "Three.md", after: null },
						{ name: "One.md", after: "Two.md" },
					],
				}),
			).what,
		).toBe(
			"Order changed: “Three” moved to the top, “One” moved after “Two”",
		);
	});
});

describe("label", () => {
	it("is a version's name, or its date and time", () => {
		const at = new Date(2026, 8, 12, 9, 5);
		expect(label(version(at, { name: "Draft one" }))).toBe("Draft one");
		expect(label(version(at))).toBe(
			`${when(at.toISOString())}, ${timeOfDay(at.toISOString())}`,
		);
	});
});
