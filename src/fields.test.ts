import { createHeadlessEditor } from "@lexical/headless";
import { describe, expect, test } from "vitest";
import { $fields, $setField } from "./fields";
import { $fromMarkdown, $toMarkdown, EDITOR_NODES } from "./markdown";
import type { Value } from "./frontmatter";

// A headless editor holds the same root state a mounted one does, so the
// fields can be set and read back without React or a file on disk.
function open(markdown: string) {
	const editor = createHeadlessEditor({
		namespace: "aurora",
		nodes: EDITOR_NODES,
		onError: (error) => {
			throw error;
		},
	});
	editor.update(() => $fromMarkdown(markdown), { discrete: true });
	return editor;
}

/** The file that would be written after setting each field in turn. */
function written(markdown: string, ...edits: [string, Value | null][]): string {
	const editor = open(markdown);

	for (const [key, value] of edits) {
		editor.update(() => $setField(key, value), { discrete: true });
	}

	return editor.getEditorState().read(() => $toMarkdown());
}

describe("fields on an open document", () => {
	test("a document with no block reads no fields", () => {
		const fields = open("Sing to me of the man.")
			.getEditorState()
			.read(() => $fields());

		expect(fields).toEqual(new Map());
	});

	test("the block a file arrived with is read as fields", () => {
		const fields = open(
			"---\ntags:\n  - homecoming\nsynopsis: She comes home.\n---\n\nSing to me.",
		)
			.getEditorState()
			.read(() => $fields());

		expect(fields).toEqual(
			new Map<string, Value>([
				["tags", ["homecoming"]],
				["synopsis", "She comes home."],
			]),
		);
	});

	test("a field set on a document reaches the markdown", () => {
		expect(
			written("Sing to me of the man.", ["tags", ["homecoming"]]),
		).toBe("---\ntags:\n  - homecoming\n---\n\nSing to me of the man.");
	});

	test("setting a field leaves the prose alone", () => {
		const prose = "# Chapter One\n\nShe was *late*, and **very** cross.";

		expect(written(prose, ["synopsis", "She comes home."])).toBe(
			`---\nsynopsis: She comes home.\n---\n\n${prose}`,
		);
	});

	test("a shape Aurora does not understand stays in the block", () => {
		expect(
			written("---\nplaces:\n  ithaca: rough\n---\n\nSing to me.", [
				"tags",
				["sea"],
			]),
		).toBe(
			"---\nplaces:\n  ithaca: rough\ntags:\n  - sea\n---\n\nSing to me.",
		);
	});

	test("dropping the last field drops the block with it", () => {
		expect(
			written("---\ntags:\n  - sea\n---\n\nSing to me.", ["tags", null]),
		).toBe("Sing to me.");
	});
});
