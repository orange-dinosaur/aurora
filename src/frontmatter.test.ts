import { describe, expect, test } from "vitest";
import { list, parse, serialize, text, type Fields } from "./frontmatter";

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
