import { describe, expect, test } from "vitest";
import {
	custom,
	fieldNames,
	list,
	parse,
	serialize,
	split,
	text,
	ties,
	type Fields,
} from "./frontmatter";

/** A block as it sits at the top of a file, fences and all. */
function block(...lines: string[]): string {
	return ["---", ...lines, "---"].join("\n");
}

function fields(entries: [string, string | string[]][]): Fields {
	return new Map(entries);
}

describe("reading front matter", () => {
	test("a block with nothing in it sets no fields", () => {
		expect(parse("")).toEqual(new Map());
		expect(parse(block())).toEqual(new Map());
	});

	test("a field holding one line is text", () => {
		expect(parse(block("synopsis: Elena comes home."))).toEqual(
			fields([["synopsis", "Elena comes home."]]),
		);
	});

	test("a quoted field keeps what the quotes hold", () => {
		const found = parse(
			block(`title: 'Elena''s return'`, `remarks: "he said \\"no\\""`),
		);

		expect(found.get("title")).toBe("Elena's return");
		expect(found.get("remarks")).toBe('he said "no"');
	});

	test("a field holding several lines is a list", () => {
		expect(parse(block("tags:", "  - homecoming", "  - sea"))).toEqual(
			fields([["tags", ["homecoming", "sea"]]]),
		);
	});

	test("a list written on one line reads the same way", () => {
		expect(parse(block("tags: [homecoming, 'sea']"))).toEqual(
			fields([["tags", ["homecoming", "sea"]]]),
		);
		expect(parse(block("tags: []"))).toEqual(fields([["tags", []]]));
	});

	test("a shape Aurora does not understand sets no field", () => {
		const found = parse(
			block(
				"# a note to myself",
				"places:",
				"  ithaca:",
				"    weather: rough",
				"draft: |",
				"  She comes home.",
				"synopsis: She comes home.",
			),
		);

		expect(found).toEqual(fields([["synopsis", "She comes home."]]));
	});
});

describe("reading a field whatever shape it is in", () => {
	test("a field that is not there reads as empty", () => {
		expect(text(new Map(), "synopsis")).toBe("");
		expect(list(new Map(), "tags")).toEqual([]);
	});

	test("text asked for as a list is the one item", () => {
		expect(list(fields([["tags", "sea"]]), "tags")).toEqual(["sea"]);
		expect(list(fields([["tags", ""]]), "tags")).toEqual([]);
	});

	test("a list asked for as text is joined", () => {
		expect(text(fields([["synopsis", ["One.", "Two."]]]), "synopsis")).toBe(
			"One., Two.",
		);
	});
});

describe("taking a block off a file", () => {
	test("a file without one is all prose", () => {
		expect(split("Sing to me.")).toEqual({
			block: "",
			body: "Sing to me.",
		});
	});

	test("the block keeps its fences and the prose loses them", () => {
		expect(split("---\ntags: [sea]\n---\n\nSing to me.")).toEqual({
			block: "---\ntags: [sea]\n---",
			body: "\nSing to me.",
		});
	});
});

describe("fields the writer added", () => {
	test("the four Aurora knows are not among them", () => {
		const found = parse(
			block("tags: [sea]", "synopsis: She comes home.", "fear: heights"),
		);

		expect(custom(found)).toEqual(["fear"]);
	});

	test("names are gathered across the project and sorted", () => {
		expect(
			fieldNames([
				"---\nfear: heights\n---\n\nOne.",
				"---\nage: 40\nfear: water\ntags: [sea]\n---\n\nTwo.",
				"No block at all.",
			]),
		).toEqual(["age", "fear"]);
	});
});

describe("writing front matter", () => {
	test("no fields at all is no block at all", () => {
		expect(serialize(new Map())).toBe("");
	});

	test("a new field is written in Aurora's own spelling", () => {
		expect(
			serialize(
				fields([
					["synopsis", "Elena comes home."],
					["tags", ["homecoming", "sea"]],
					["names", []],
				]),
			),
		).toBe(
			block(
				"synopsis: Elena comes home.",
				"tags:",
				"  - homecoming",
				"  - sea",
				"names: []",
			),
		);
	});

	test("a value YAML would read as something else is quoted", () => {
		expect(
			serialize(
				fields([
					["a", "true"],
					["b", "3"],
					["c", "Elena: the Captain"],
					["d", ""],
					["e", "- not a list"],
				]),
			),
		).toBe(
			block(
				`a: "true"`,
				`b: "3"`,
				`c: "Elena: the Captain"`,
				`d: ""`,
				`e: "- not a list"`,
			),
		);
	});

	test("a value with a line break survives the trip", () => {
		const written = serialize(fields([["remarks", "One.\nTwo."]]));

		expect(written).toBe(block(`remarks: "One.\\nTwo."`));
		expect(parse(written).get("remarks")).toBe("One.\nTwo.");
	});

	test("writing back what was read changes nothing", () => {
		const original = block(
			"# a note to myself",
			`title: 'Elena''s return'`,
			"",
			"tags:  [homecoming]",
			"places:",
			"  ithaca:",
			"    weather: rough",
		);

		expect(serialize(parse(original), original)).toBe(original);
	});

	test("only the field that changed is rewritten", () => {
		const original = block(
			`title: 'Elena''s return'`,
			"tags:  [homecoming]",
			"places:",
			"  ithaca:",
			"    weather: rough",
		);

		const now = parse(original);
		now.set("tags", ["sea"]);

		expect(serialize(now, original)).toBe(
			block(
				`title: 'Elena''s return'`,
				"tags:",
				"  - sea",
				"places:",
				"  ithaca:",
				"    weather: rough",
			),
		);
	});

	test("a field that is gone takes its lines with it", () => {
		const original = block("synopsis: She comes home.", "tags:", "  - sea");

		const now = parse(original);
		now.delete("tags");

		expect(serialize(now, original)).toBe(
			block("synopsis: She comes home."),
		);
	});

	test("a field the block never had goes on the end", () => {
		const original = block("synopsis: She comes home.");

		const now = parse(original);
		now.set("names", ["Ellie"]);

		expect(serialize(now, original)).toBe(
			block("synopsis: She comes home.", "names:", "  - Ellie"),
		);
	});
});

describe("relationships", () => {
	test("a block of pairs is read as pairs", () => {
		expect(
			parse(
				block(
					"relationships:",
					"  Ana Ferrer: her older sister",
					"  Bruno: the neighbour who knows",
				),
			),
		).toEqual(
			new Map([
				[
					"relationships",
					new Map([
						["Ana Ferrer", "her older sister"],
						["Bruno", "the neighbour who knows"],
					]),
				],
			]),
		);
	});

	test("a name with nothing said about it yet is still a tie", () => {
		expect(parse(block("relationships:", "  Bruno:"))).toEqual(
			new Map([["relationships", new Map([["Bruno", ""]])]]),
		);
	});

	test("a map deeper than one level is still left alone", () => {
		const original = block(
			"relationships:",
			"  ithaca:",
			"    weather: rough",
		);

		expect(parse(original)).toEqual(new Map());
		expect(serialize(parse(original), original)).toBe(original);
	});

	// A block only ever holds one shape. The lines that do not fit end it,
	// which is what any other stray line under a key has always done, and they
	// are kept verbatim rather than read.
	test("a block that mixes items and pairs keeps the shape it opened with", () => {
		const original = block(
			"relationships:",
			"  - Ana Ferrer",
			"  Bruno: the neighbour",
		);

		expect(parse(original)).toEqual(
			new Map([["relationships", ["Ana Ferrer"]]]),
		);
		expect(serialize(parse(original), original)).toBe(original);
	});

	test("a pair block that turns into something else is left alone", () => {
		const original = block(
			"relationships:",
			"  Ana Ferrer: her older sister",
			"  - Bruno",
		);

		expect(parse(original)).toEqual(
			new Map([
				[
					"relationships",
					new Map([["Ana Ferrer", "her older sister"]]),
				],
			]),
		);
		expect(serialize(parse(original), original)).toBe(original);
	});

	test("pairs the writer wrote come back exactly as written", () => {
		const original = block(
			"title: Isolde",
			"relationships:",
			"  Ana Ferrer:   her older sister",
			"  Bruno: the neighbour who knows",
		);

		expect(serialize(parse(original), original)).toBe(original);
	});

	test("a tie that changed is rewritten and the rest is not", () => {
		const original = block(
			"title: Isolde",
			"relationships:",
			"  Ana Ferrer:   her older sister",
		);

		const now = parse(original);
		now.set("relationships", new Map([["Ana Ferrer", "her twin"]]));

		expect(serialize(now, original)).toBe(
			block("title: Isolde", "relationships:", "  Ana Ferrer: her twin"),
		);
	});

	test("a name that needs quoting gets it on the way out", () => {
		const fields: Fields = new Map([
			["relationships", new Map([["Ana: of the north", "her sister"]])],
		]);

		expect(serialize(fields)).toBe(
			block("relationships:", `  "Ana: of the north": her sister`),
		);
	});

	test("reordering counts as a change", () => {
		const original = block(
			"relationships:",
			"  Ana: her sister",
			"  Bruno: the neighbour",
		);

		const now = parse(original);
		now.set(
			"relationships",
			new Map([
				["Bruno", "the neighbour"],
				["Ana", "her sister"],
			]),
		);

		expect(serialize(now, original)).toBe(
			block(
				"relationships:",
				"  Bruno: the neighbour",
				"  Ana: her sister",
			),
		);
	});

	test("ties reads whatever shape the file gave the field", () => {
		expect(
			ties(
				parse(block("relationships:", "  Ana: her sister")),
				"relationships",
			),
		).toEqual(new Map([["Ana", "her sister"]]));

		// A writer who wrote a plain list gets the names with nothing said.
		expect(
			ties(
				parse(block("relationships:", "  - Ana", "  - Bruno")),
				"relationships",
			),
		).toEqual(
			new Map([
				["Ana", ""],
				["Bruno", ""],
			]),
		);

		expect(ties(new Map(), "relationships")).toEqual(new Map());
	});

	test("a field of pairs read as text or a list says both halves", () => {
		const found = parse(block("relationships:", "  Ana: her sister"));

		expect(list(found, "relationships")).toEqual(["Ana: her sister"]);
		expect(text(found, "relationships")).toBe("Ana: her sister");
	});
});
