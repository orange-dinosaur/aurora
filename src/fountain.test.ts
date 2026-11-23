import { describe, expect, test } from "vitest";
import {
	classify,
	cycled,
	emphasised,
	follows,
	spans,
	type Span,
	overrules,
	parse,
	serialise,
} from "./fountain";
import type { Block, BlockKind } from "./fountain";
import cases from "./fountain.fixture.json";

type Case = { name: string; text: string; canonical?: string; blocks: Block[] };

describe("reading and writing Fountain", () => {
	test.each(cases as Case[])("$name", ({ text, canonical, blocks }) => {
		expect(parse(text)).toEqual(blocks);
		expect(serialise(blocks)).toBe(canonical ?? text);
	});
});

describe("marks follow the kind, not the block's flag", () => {
	test("a block forced by the writer's Tab gets its mark", () => {
		const blocks: Block[] = [
			{ kind: "character", text: "Rose", forced: false },
			{ kind: "dialogue", text: "Hello.", forced: false },
		];
		expect(serialise(blocks)).toBe("@Rose\nHello.");
	});

	test("a cue whose dialogue starts with capitals keeps its dialogue", () => {
		const blocks: Block[] = [
			{ kind: "character", text: "ROSE", forced: false },
			{ kind: "dialogue", text: "NO!", forced: false },
		];
		expect(parse(serialise(blocks))).toEqual(blocks);
	});
});

describe("blocks follow their text as the writer types", () => {
	const typed = (...lines: [BlockKind, string][]): Block[] =>
		lines.map(([kind, text]) => ({ kind, text, forced: false }));

	test("a heading, a cue and its dialogue, typed a line at a time", () => {
		const lines = ["INT. KITCHEN - DAY", "MARCUS", "(quietly)", "Hello."];
		let blocks: Block[] = [];
		for (const text of lines) {
			blocks = [...blocks, { kind: "action", text, forced: false }];
			const kinds = classify(blocks);
			blocks = blocks.map((block, i) => ({ ...block, kind: kinds[i] }));
		}
		expect(blocks.map((block) => block.kind)).toEqual([
			"heading",
			"character",
			"parenthetical",
			"dialogue",
		]);
	});

	test("a capitalised line at the end is a cue until text follows", () => {
		expect(classify(typed(["action", "ROSE"]))).toEqual(["character"]);
		expect(classify(typed(["action", "ROSE"], ["action", ""]))).toEqual([
			"character",
			"action",
		]);
	});

	test("a cue keeps its kind once dialogue is typed under it", () => {
		expect(
			classify(typed(["character", "ROSE"], ["action", "Hi."])),
		).toEqual(["character", "dialogue"]);
	});

	test("capitals above action stay action", () => {
		expect(
			classify(typed(["action", "BANG!"], ["action", "He runs."])),
		).toEqual(["action", "action"]);
	});

	test("action after dialogue does not join the speech", () => {
		expect(
			classify(
				typed(
					["character", "ROSE"],
					["dialogue", "Hi."],
					["action", "She leaves."],
				),
			),
		).toEqual(["character", "dialogue", "action"]);
	});

	test("forced blocks and mark-only kinds keep their kind", () => {
		const blocks: Block[] = [
			{ kind: "action", text: "INT. HALL", forced: true },
			{ kind: "centered", text: "THE END", forced: false },
		];
		expect(classify(blocks)).toEqual(["action", "centered"]);
	});
});

describe("Enter and Tab", () => {
	test("Enter goes from a cue to dialogue and from dialogue to action", () => {
		expect(follows("character")).toBe("dialogue");
		expect(follows("parenthetical")).toBe("dialogue");
		expect(follows("dialogue")).toBe("action");
		expect(follows("heading")).toBe("action");
		expect(follows("transition")).toBe("heading");
	});

	test("Tab cycles both ways and wraps", () => {
		expect(cycled("heading", 1)).toBe("action");
		expect(cycled("action", 1)).toBe("character");
		expect(cycled("transition", 1)).toBe("heading");
		expect(cycled("heading", -1)).toBe("transition");
		expect(cycled("centered", 1)).toBe("action");
	});

	test("Shift+Tab undoes Tab", () => {
		const kinds: BlockKind[] = [
			"heading",
			"action",
			"character",
			"dialogue",
			"parenthetical",
			"transition",
		];
		for (const kind of kinds) {
			expect(cycled(cycled(kind, 1), -1)).toBe(kind);
		}
	});

	test("a lowercase name made a cue is forced", () => {
		const blocks: Block[] = [
			{ kind: "action", text: "marcus", forced: false },
		];
		expect(overrules(blocks, 0, "character")).toBe(true);
	});

	test("a choice the text already makes is not forced", () => {
		const blocks: Block[] = [
			{ kind: "character", text: "ROSE", forced: false },
			{ kind: "action", text: "Hi.", forced: false },
		];
		expect(overrules(blocks, 1, "dialogue")).toBe(false);
	});
});

describe("emphasis", () => {
	const plain = { bold: false, italic: false, underline: false };
	const span = (text: string, styles: Partial<Span> = {}): Span => ({
		...plain,
		text,
		...styles,
	});

	test.each<[string, Span[]]>([
		["plain words", [span("plain words")]],
		[
			"a **bold** word",
			[span("a "), span("bold", { bold: true }), span(" word")],
		],
		[
			"*italic* and _under_",
			[
				span("italic", { italic: true }),
				span(" and "),
				span("under", { underline: true }),
			],
		],
		["***both***", [span("both", { bold: true, italic: true })]],
		[
			"**a*b***",
			[
				span("a", { bold: true }),
				span("b", { bold: true, italic: true }),
			],
		],
		[
			"_**all *three***_",
			[
				span("all ", { underline: true, bold: true }),
				span("three", { underline: true, bold: true, italic: true }),
			],
		],
		["5 \\* 3 and snake\\_case", [span("5 * 3 and snake_case")]],
		["C:\\\\\\*", [span("C:\\*")]],
	])("%s round-trips", (text, line) => {
		expect(spans(text)).toEqual(line);
		expect(emphasised(line)).toBe(text);
	});

	test("a marker with no partner is a character", () => {
		expect(spans("5 * 3")).toEqual([span("5 * 3")]);
		expect(emphasised(spans("5 * 3"))).toBe("5 \\* 3");
	});

	test("a backslash that escapes nothing stays", () => {
		expect(spans("C:\\path")).toEqual([span("C:\\path")]);
		expect(emphasised([span("C:\\path")])).toBe("C:\\path");
	});

	test("a backslash before emphasis is not read as an escape", () => {
		const line = [span("a\\"), span("b", { bold: true })];
		expect(spans(emphasised(line))).toEqual(line);
	});
});
