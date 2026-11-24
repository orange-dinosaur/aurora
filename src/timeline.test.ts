import { describe, expect, test } from "vitest";
import type { Mentionable } from "./lib/mentions";
import { runsOf } from "./runs";
import { rowsOf, subjectsOf, type TimelineRow } from "./timeline";

function page(section: string, title: string, ...names: string[]): Mentionable {
	return {
		id: `${section}/${title}.md`,
		title,
		trail: [section],
		runs: runsOf(`# ${title}`),
		tags: [],
		names,
	};
}

function scene(id: string, text: string, tags: string[] = []): Mentionable {
	return {
		id,
		title: id,
		trail: ["Manuscript"],
		runs: runsOf(text),
		tags,
		names: [],
	};
}

/** Each row as its subject's title, or "-" for the scenes naming no one. */
function drawn(rows: TimelineRow[]): [string, string[]][] {
	return rows.map((row) => [row.subject?.title ?? "-", [...row.scenes]]);
}

const cast = [
	page("Characters", "Elena"),
	page("Characters", "Rose", "Rosie"),
	page("Locations", "Harbour"),
];

describe("timeline rows", () => {
	test("a scene naming two characters sits in both rows", () => {
		const documents = [...cast, scene("one", "Elena met Rose.")];

		expect(drawn(rowsOf(["one"], documents, "Characters", false))).toEqual([
			["Elena", ["one"]],
			["Rose", ["one"]],
		]);
	});

	test("rows come in the order their subject is first met in story order", () => {
		const documents = [
			...cast,
			scene("one", "Rose waited."),
			scene("two", "Elena came, and Rosie too."),
		];

		expect(
			drawn(rowsOf(["two", "one"], documents, "Characters", false)),
		).toEqual([
			["Elena", ["two"]],
			["Rose", ["two", "one"]],
		]);
	});

	test("scenes naming no one go in a last row, which is left out when there are none", () => {
		const documents = [
			...cast,
			scene("one", "The sea rose."),
			scene("two", "Elena slept."),
		];

		expect(
			drawn(rowsOf(["one", "two"], documents, "Characters", false)),
		).toEqual([
			["Elena", ["two"]],
			["-", ["one"]],
		]);
		expect(drawn(rowsOf(["two"], documents, "Characters", false))).toEqual([
			["Elena", ["two"]],
		]);
	});

	test("only the section asked for makes rows", () => {
		const documents = [...cast, scene("one", "Elena at the Harbour.")];

		expect(drawn(rowsOf(["one"], documents, "Locations", false))).toEqual([
			["Harbour", ["one"]],
		]);
	});

	test("a tag puts a scene in a row as a name does", () => {
		const documents = [...cast, scene("one", "Nobody spoke.", ["Rose"])];

		expect(drawn(rowsOf(["one"], documents, "Characters", false))).toEqual([
			["Rose", ["one"]],
		]);
	});

	test("a screenplay's cue in capitals counts", () => {
		const documents = [...cast, scene("one", "ELENA\nHello.")];

		expect(drawn(rowsOf(["one"], documents, "Characters", true))).toEqual([
			["Elena", ["one"]],
		]);
	});

	test("a hidden subject has no row, and its scenes do not move to the last one", () => {
		const documents = [
			...cast,
			scene("one", "Elena met Rose."),
			scene("two", "Rose alone."),
		];
		const hidden = new Set([cast[1].id]);

		expect(
			drawn(
				rowsOf(["one", "two"], documents, "Characters", false, hidden),
			),
		).toEqual([["Elena", ["one"]]]);
	});

	test("the row list offers every page in the section, by title", () => {
		const documents = [page("Characters", "Zoe"), ...cast];

		expect(
			subjectsOf(documents, "Characters").map((subject) => subject.title),
		).toEqual(["Elena", "Rose", "Zoe"]);
	});

	test("a scene the project has not read yet names no one", () => {
		expect(drawn(rowsOf(["gone"], cast, "Characters", false))).toEqual([
			["-", ["gone"]],
		]);
	});
});
