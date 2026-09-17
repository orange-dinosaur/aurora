import { describe, expect, test } from "vitest";
import { flushAll, holdUnsaved } from "./flushing";

describe("flushing every screen that holds unsaved text", () => {
	test("both of two held flushes run", async () => {
		const written: string[] = [];
		const letGoOfOne = holdUnsaved(async () => {
			written.push("one");
		});
		const letGoOfTwo = holdUnsaved(async () => {
			written.push("two");
		});

		await flushAll();
		letGoOfOne();
		letGoOfTwo();

		expect(written.sort()).toEqual(["one", "two"]);
	});

	test("a flush that was let go does not run", async () => {
		const written: string[] = [];
		const letGoOfGone = holdUnsaved(async () => {
			written.push("gone");
		});
		const letGoOfHere = holdUnsaved(async () => {
			written.push("here");
		});

		letGoOfGone();
		await flushAll();
		letGoOfHere();

		expect(written).toEqual(["here"]);
	});
});
