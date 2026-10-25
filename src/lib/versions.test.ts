import { describe, expect, it } from "vitest";
import { timeOfDay, when } from "../dates";
import type { Change, Checkpoint, Merging } from "../types";
import {
	byDay,
	counts,
	detail,
	lineKey,
	lines,
	overwriteQuestion,
	overwritten,
	settleBanner,
	settleFirst,
	settleLines,
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

describe("overwritten", () => {
	function node(id: string, rest: Partial<Change> = {}): Change {
		return {
			id,
			folder: false,
			before: `Manuscript/${id}.md`,
			after: `Manuscript/${id}.md`,
			renamed: false,
			moved: false,
			edited: true,
			reordered: [],
			...rest,
		};
	}
	// Mill is partly in the next checkpoint, so it is in both lists.
	const status = {
		staged: { nodes: [node("Mill")], book: false, cover: true },
		unstaged: {
			nodes: [
				node("Mill"),
				node("Letter"),
				node("Part", { folder: true, before: "Part", after: "Part" }),
			],
			book: true,
			cover: false,
		},
	};

	it("names each document once, with the book and cover, never a folder", () => {
		expect(overwritten(status, { kind: "everything" })).toEqual([
			"The cover",
			"Mill",
			"The book",
			"Letter",
		]);
	});

	it("narrows to one document, or to the book and cover", () => {
		expect(overwritten(status, { kind: "document", id: "Letter" })).toEqual(
			["Letter"],
		);
		expect(overwritten(status, { kind: "document", id: "Night" })).toEqual(
			[],
		);
		expect(overwritten(status, { kind: "book" })).toEqual([
			"The cover",
			"The book",
		]);
	});

	it("asks by name for one and by count for more", () => {
		expect(overwriteQuestion(["Mill"])).toBe(
			"Mill has changes that aren't in a checkpoint. Putting back overwrites them.",
		);
		expect(overwriteQuestion(["Mill", "Letter"])).toBe(
			"2 documents have changes that aren't in a checkpoint. Putting back overwrites them.",
		);
	});
});

describe("settleLines", () => {
	const merging = (conflicts: Merging["conflicts"]): Merging => ({
		name: "Second pass",
		message: "Merged Second pass",
		conflicts,
	});
	const text = (id: string | null, path: string) =>
		({ kind: "text", id, path }) as const;

	it("marks each conflicted document with its spots, then one binder row", () => {
		const rows = settleLines(
			merging([
				text("s2", "Manuscript/Chapter 1/Scene 2.md"),
				{ kind: "renamed", id: "s3", ours: "A.md", theirs: "B.md" },
				text("s7", "Manuscript/Chapter 2/Scene 7.md"),
				{ kind: "cover", ours: "cover.png", theirs: null },
			]),
			{ s2: 3, s7: 1 },
		);

		expect(rows.map(({ name, what, kind }) => [name, what, kind])).toEqual([
			["Scene 2", "3 spots", "document"],
			["Scene 7", "1 spot", "document"],
			["Binder and details", "2 to settle", "binder"],
		]);
		expect(rows.map((row) => row.id)).toEqual(["s2", "s7", null]);
	});

	it("has no binder row with only text to settle", () => {
		const rows = settleLines(merging([text("s2", "Notes/Scene 2.md")]), {
			s2: 2,
		});
		expect(rows.map((row) => row.kind)).toEqual(["document"]);
	});

	it("leaves the count off until it is known, and for a file with no id", () => {
		const rows = settleLines(
			merging([text("s2", "Notes/Scene 2.md"), text(null, "notes.txt")]),
			{},
		);
		expect(rows.map(({ name, what, id }) => [name, what, id])).toEqual([
			["Scene 2", "", "s2"],
			["notes.txt", "", null],
		]);
	});

	it("is empty once everything is settled", () => {
		expect(settleLines(merging([]), {})).toEqual([]);
	});
});

describe("settle wording", () => {
	it("counts the rows left, in the singular for one", () => {
		expect(settleBanner(3)).toBe(
			"3 things to settle before the merge can be a checkpoint.",
		);
		expect(settleBanner(1)).toBe(
			"1 thing to settle before the merge can be a checkpoint.",
		);
		expect(settleBanner(0)).toBe(
			"Everything is settled. Add the checkpoint to finish the merge.",
		);
		expect(settleFirst(3)).toBe("Settle the 3 marked rows first.");
		expect(settleFirst(1)).toBe("Settle the marked row first.");
	});
});
