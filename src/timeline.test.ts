import { describe, expect, test } from "vitest";
import type { Mentionable } from "./lib/mentions";
import { runsOf } from "./runs";
import {
	chainOf,
	dropAt,
	gapAt,
	hue,
	markerAt,
	markerStepped,
	rowsOf,
	stepped,
	stretchesOf,
	subjectsOf,
	type TimelineRow,
} from "./timeline";

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

describe("dropping a scene", () => {
	const order = ["a", "b", "c", "d"];

	test("a drop lands in the nearest gap, after the scene on its left", () => {
		expect(dropAt(260, 100, order, "a", {})).toEqual({
			gap: 3,
			place: { after: "c" },
			before: "d",
		});
		expect(dropAt(140, 100, order, "d", {})).toEqual({
			gap: 1,
			place: { after: "a" },
			before: "b",
		});
	});

	test("a drop before the first scene puts it first", () => {
		expect(dropAt(-40, 100, order, "c", {})).toEqual({
			gap: 0,
			place: "first",
			before: "a",
		});
	});

	test("a drop past the last scene has nothing on its right", () => {
		expect(dropAt(900, 100, order, "a", {})).toEqual({
			gap: 4,
			place: { after: "d" },
			before: null,
		});
	});

	test("either side of the scene itself is no drop", () => {
		expect(dropAt(100, 100, order, "b", {})).toBeNull();
		expect(dropAt(200, 100, order, "b", {})).toBeNull();
	});

	test("no gap inside the scene's chain of followers", () => {
		const placed = { c: { after: "b" }, d: { after: "c" } };

		expect([...chainOf("b", placed)]).toEqual(["b", "c", "d"]);
		expect(dropAt(300, 100, order, "b", placed)).toBeNull();
		expect(dropAt(400, 100, order, "b", placed)).toBeNull();
		expect(dropAt(0, 100, order, "b", placed)?.place).toBe("first");
	});

	test("a scene placed first is no follower", () => {
		expect([...chainOf("a", { b: "first" })]).toEqual(["a"]);
	});
});

describe("markers", () => {
	const order = ["a", "b", "c", "d", "e"];
	const later = { id: "m2", name: "Three years later", before: "d" };
	const summer = { id: "m1", name: "Summer", before: "a" };
	const gone = { id: "m3", name: "Cut", before: "zz" };

	test("a marker runs from its scene to the next marker or the end", () => {
		expect(
			stretchesOf([later, gone, summer], order).map(
				({ marker, from, to }) => [marker.name, from, to],
			),
		).toEqual([
			["Summer", 0, 3],
			["Three years later", 3, 5],
		]);
	});

	test("a marker goes before the scene right of the gap", () => {
		const stretches = stretchesOf([later], order);

		expect(markerAt(gapAt(140, 100, order.length), order, stretches)).toBe(
			"b",
		);
		expect(markerAt(0, order, stretches)).toBe("a");
	});

	test("no marker past the last scene or where one already starts", () => {
		const stretches = stretchesOf([later], order);

		expect(markerAt(5, order, stretches)).toBeNull();
		expect(markerAt(3, order, stretches)).toBeNull();
	});

	test("a marker steps one scene each way, but not past an end or another marker", () => {
		const stretches = stretchesOf([summer, later], order);
		const [first, second] = stretches;

		expect(markerStepped(first, -1, order, stretches)).toBeNull();
		expect(markerStepped(first, 1, order, stretches)).toBe("b");
		expect(markerStepped(second, -1, order, stretches)).toBe("c");
		expect(markerStepped(second, 1, order, stretches)).toBe("e");

		const last = stretchesOf([{ ...later, before: "e" }], order);
		expect(markerStepped(last[0], 1, order, last)).toBeNull();

		const touching = stretchesOf(
			[summer, { ...later, before: "b" }],
			order,
		);
		expect(markerStepped(touching[0], 1, order, touching)).toBeNull();
		expect(markerStepped(touching[1], -1, order, touching)).toBeNull();
	});
});

describe("moving a scene without a drag", () => {
	const order = ["a", "b", "c", "d"];

	test("one step lands the gap past the neighbour, as a drop there would", () => {
		expect(stepped(order, "c", {}, -1)).toEqual({
			gap: 1,
			place: { after: "a" },
			before: "b",
		});
		expect(stepped(order, "b", {}, 1)).toEqual({
			gap: 3,
			place: { after: "c" },
			before: "d",
		});
	});

	test("the first scene steps to first and the last to the end", () => {
		expect(stepped(order, "b", {}, -1)?.place).toBe("first");
		expect(stepped(order, "c", {}, 1)).toEqual({
			gap: 4,
			place: { after: "d" },
			before: null,
		});
	});

	test("nothing further at either end", () => {
		expect(stepped(order, "a", {}, -1)).toBeNull();
		expect(stepped(order, "d", {}, 1)).toBeNull();
		expect(stepped(order, "zz", {}, 1)).toBeNull();
	});

	test("a later step goes past the scene's own followers", () => {
		const placed = { c: { after: "b" } };

		expect(stepped(order, "b", placed, 1)).toEqual({
			gap: 4,
			place: { after: "d" },
			before: null,
		});
		expect(
			stepped(
				order,
				"a",
				{ b: { after: "a" }, d: { after: "c" }, ...placed },
				1,
			),
		).toBeNull();
	});
});

describe("block colours", () => {
	test("rows take the five colours in turn", () => {
		expect([0, 1, 4, 5, 9].map(hue)).toEqual([1, 2, 5, 1, 5]);
	});
});
