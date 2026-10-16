import { describe, expect, it } from "vitest";
import { unmergedQuestion } from "./branches";

describe("unmergedQuestion", () => {
	it("counts the checkpoints deleting would lose", () => {
		expect(unmergedQuestion("Act two rewrite", 3)).toBe(
			"Act two rewrite has 3 checkpoints not merged into any other version. Delete it and them?",
		);
	});

	it("speaks of one checkpoint in the singular", () => {
		expect(unmergedQuestion("Aside", 1)).toBe(
			"Aside has 1 checkpoint not merged into any other version. Delete it and the checkpoint?",
		);
	});
});
