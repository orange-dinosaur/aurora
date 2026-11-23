import { describe, expect, test } from "vitest";
import { parse, serialise } from "./fountain";
import type { Block } from "./fountain";
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
