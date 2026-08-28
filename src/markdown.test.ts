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
		["a scene break", "One.\n\n---\n\nTwo."],
		["a nested list", "- one\n    - inner\n- two"],
		["a nested numbered list", "1. one\n    1. inner\n2. two"],
		["several blocks together", "# Chapter One\n\nShe *ran*.\n\n- a\n- b"],
	])("keeps %s", (_what, markdown) => {
		expect(roundTrip(markdown)).toBe(markdown);
	});
});

// Markdown a writer might paste in that Aurora has no node for. It has to come
// back out unharmed even though nothing understands it.
describe("what Aurora cannot parse is left alone", () => {
	test.each([
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

	// Lexical measures one level of nesting as four spaces. Two-space
	// indentation is common enough elsewhere that this is worth stating: the
	// nesting is lost, though only once, and never on a file Aurora wrote.
	test("a list nested by two spaces is flattened", () => {
		expect(roundTrip("- one\n  - inner\n- two")).toBe(
			"- one\n- inner\n- two",
		);
	});

	test("every spelling of a scene break is written back as three dashes", () => {
		for (const written of ["***", "___", "* * *", "- - -", "-----"]) {
			expect(roundTrip(`One.\n\n${written}\n\nTwo.`)).toBe(
				"One.\n\n---\n\nTwo.",
			);
		}
	});

	test("a list indented with a tab comes back indented with spaces", () => {
		expect(roundTrip("- one\n\t- inner\n- two")).toBe(
			"- one\n    - inner\n- two",
		);
	});
});

// Some note-taking apps fence a block of settings off at the top of a file
// with the same three dashes that mark a scene break. Aurora holds it aside
// rather than reading it, so both meanings can coexist.
describe("front matter is set aside and put back", () => {
	test.each([
		["on its own", "---\ntitle: The Sea\n---\n\nProse."],
		[
			"with several keys",
			"---\ntitle: The Sea\ntags: [draft]\n---\n\nProse.",
		],
		[
			"above a document that also has a scene break",
			"---\ntitle: The Sea\n---\n\nOne.\n\n---\n\nTwo.",
		],
		["with nothing after it", "---\ntitle: The Sea\n---"],
	])("survives %s", (_what, markdown) => {
		expect(roundTrip(markdown)).toBe(markdown);
	});

	test("three dashes with no closing fence are a scene break, not settings", () => {
		expect(roundTrip("---\n\nProse.")).toBe("---\n\nProse.");
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
		"- one\n    - inner\n        - deeper\n- two",
		"---\ntitle: The Sea\n---\n\nProse.",
		"---\ntitle: The Sea\n---\n\nOne.\n\n***\n\nTwo.",
		"| a | b |\n| --- | --- |\n| 1 | 2 |",
		"Text[^1]\n\n[^1]: A note.",
	])("settles for %j", (markdown) => {
		const once = roundTrip(markdown);
		expect(roundTrip(once)).toBe(once);
	});
});
