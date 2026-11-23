import { describe, expect, test } from "vitest";
import { classify, parse, serialise } from "./fountain";
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
