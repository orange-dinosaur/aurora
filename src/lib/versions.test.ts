import { describe, expect, it } from "vitest";
import { timeOfDay, when } from "../dates";
import type { Change, Checkpoint } from "../types";
import {
	byDay,
	counts,
	detail,
	label,
	lineKey,
	lines,
	summary,
} from "./versions";

function checkpoint(at: Date, rest: Partial<Checkpoint> = {}): Checkpoint {
	return {
		id: at.toISOString(),
		at: at.toISOString(),
		message: "Draft one",
		author: "Ines",
		words: null,
		...rest,
	};
}

describe("byDay", () => {
	const now = new Date(2026, 8, 23, 18, 0);

	it("heads today and yesterday by name and older days by date", () => {
		const older = new Date(2026, 8, 12, 9, 0);
		const list = [
			checkpoint(new Date(2026, 8, 23, 15, 40)),
			checkpoint(new Date(2026, 8, 23, 9, 15)),
			checkpoint(new Date(2026, 8, 22, 20, 0)),
			checkpoint(older),
		];

		const days = byDay(list, now);

		expect(days.map((day) => day.heading)).toEqual([
			"Today",
			"Yesterday",
			when(older.toISOString()),
		]);
		expect(days.map((day) => day.checkpoints.length)).toEqual([2, 1, 1]);
	});

	it("gives an empty list no days", () => {
		expect(byDay([], now)).toEqual([]);
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

	it("gives the time and the author, and the words when it has them", () => {
		expect(detail(checkpoint(at))).toBe(`${time} · Ines`);
		expect(detail(checkpoint(at, { words: 1148 }))).toBe(
			`${time} · Ines · ${(1148).toLocaleString()} words`,
		);
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

describe("lines", () => {
	it("lists the book and cover first, then each node as its own row", () => {
		const scene: Change = {
			id: "scene",
			folder: false,
			before: null,
			after: "Manuscript/Scene.md",
			renamed: false,
			moved: false,
			edited: false,
			reordered: [],
		};
		const listed = lines({ nodes: [scene], book: true, cover: true });
		expect(listed.map(lineKey)).toEqual(["book", "cover", "scene"]);
		expect(listed.map((line) => line.row)).toEqual([
			{ kind: "book" },
			{ kind: "cover" },
			{ kind: "node", id: "scene" },
		]);
		expect(listed[2]).toMatchObject({
			name: "Scene",
			what: "New document",
			change: scene,
		});
		expect(lines({ nodes: [], book: false, cover: false })).toEqual([]);
	});
});

describe("label", () => {
	it("is the first line of the message", () => {
		const at = new Date(2026, 8, 12, 9, 5);
		expect(label(checkpoint(at))).toBe("Draft one");
		expect(
			label(checkpoint(at, { message: "Mill scene \nand the rest" })),
		).toBe("Mill scene");
	});
});
