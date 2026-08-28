import { createHeadlessEditor } from "@lexical/headless";
import { describe, expect, test } from "vitest";
import { $fromMarkdown, $toMarkdown, EDITOR_NODES } from "./markdown";

// A headless editor needs no DOM, so the conversion can be tested on its own,
// away from React and away from the file on disk.
function roundTrip(markdown: string): string {
	const editor = createHeadlessEditor({
		namespace: "aurora",
		nodes: EDITOR_NODES,
		onError: (error) => {
			throw error;
		},
	});
	editor.update(() => $fromMarkdown(markdown), { discrete: true });
	return editor.getEditorState().read(() => $toMarkdown());
}

describe("a document survives being parsed and written back", () => {
	test.each([
		["a heading", "# Chapter One"],
		["a deeper heading", "### The room above the shop"],
		["bold and italic", "She was *late*, and **very** cross."],
		["strikethrough", "It was ~~fine~~ awful."],
		["inline code", "Run `cargo test` first."],
		["a bulleted list", "- one\n- two\n- three"],
		["a numbered list", "1. one\n2. two"],
		["a blockquote", "> He never came back."],
		["a fenced code block", "```rust\nfn main() {}\n```"],
		["a link", "See [the map](https://example.com/map)."],
		["paragraphs", "One.\n\nTwo."],
		["a line break inside a paragraph", "One line.\nAnd another."],
		["several blocks together", "# Chapter One\n\nShe *ran*.\n\n- a\n- b"],
	])("keeps %s", (_what, markdown) => {
		expect(roundTrip(markdown)).toBe(markdown);
	});
});

// Markdown a writer might paste in that Aurora has no node for. It has to come
// back out unharmed even though nothing understands it.
describe("what Aurora cannot parse is left alone", () => {
	test.each([
		["front matter", "---\ntitle: The Sea\n---\n\nProse."],
		["an HTML comment", "<!-- check this name -->\n\nProse."],
		["a table", "| a | b |\n| --- | --- |\n| 1 | 2 |"],
		["a footnote", "Text[^1]\n\n[^1]: A note."],
	])("keeps %s verbatim", (_what, markdown) => {
		expect(roundTrip(markdown)).toBe(markdown);
	});
});

// These are the places the default vocabulary does change the writer's file.
// They are asserted rather than merely known, so that widening the transformers
// later has to come here and say what it fixed.
describe("what the default vocabulary changes", () => {
	test("underscores for emphasis become asterisks", () => {
		expect(roundTrip("She was _late_.")).toBe("She was *late*.");
	});

	test("literal asterisks and underscores come back escaped", () => {
		expect(roundTrip("The 5 * 3 grid, and a_b_c.")).toBe(
			"The 5 \\* 3 grid, and a\\_b\\_c.",
		);
	});

	test("a scene break of spaced asterisks becomes a list item", () => {
		expect(roundTrip("One.\n\n* * *\n\nTwo.")).toBe(
			"One.\n\n* \\* \\*\n\nTwo.",
		);
	});

	test("a nested list is flattened", () => {
		expect(roundTrip("- one\n  - inner\n- two")).toBe(
			"- one\n- inner\n- two",
		);
	});

	test("a thematic break is kept as text, not as a rule", () => {
		expect(roundTrip("One.\n\n---\n\nTwo.")).toBe("One.\n\n---\n\nTwo.");
	});
});

// The first pass may reflow a file, but the second must not: whatever Aurora
// writes, it has to read back as exactly the same thing. Without this a
// document could drift a little further every time it was opened.
describe("a second pass changes nothing", () => {
	test.each([
		"# Chapter One\n\nShe was _late_.",
		"The 5 * 3 grid.",
		"One.\n\n* * *\n\nTwo.",
		"- one\n  - inner\n- two",
		"---\ntitle: The Sea\n---\n\nProse.",
		"| a | b |\n| --- | --- |\n| 1 | 2 |",
	])("settles for %j", (markdown) => {
		const once = roundTrip(markdown);
		expect(roundTrip(once)).toBe(once);
	});
});
