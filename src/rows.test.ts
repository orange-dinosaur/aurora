import { describe, expect, test } from "vitest";
import { indent, retitling } from "./rows";

describe("which row is being retitled", () => {
	test("no row is, while the field is closed", () => {
		expect(retitling({ kind: "closed" }, "id-1")).toBeNull();
	});

	test("only the row holding the id, never the ones beside it", () => {
		const renaming = { kind: "open", id: "id-1" } as const;

		expect(retitling(renaming, "id-1")).toEqual({
			busy: false,
			error: null,
		});
		expect(retitling(renaming, "id-2")).toBeNull();
	});

	test("a name on its way to Rust leaves the field busy", () => {
		expect(retitling({ kind: "saving", id: "id-1" }, "id-1")).toEqual({
			busy: true,
			error: null,
		});
	});

	test("a name Rust turned down comes back with what it said", () => {
		const renaming = {
			kind: "refused",
			id: "id-1",
			message: "a folder of that name is already there",
		} as const;

		expect(retitling(renaming, "id-1")).toEqual({
			busy: false,
			error: "a folder of that name is already there",
		});
	});
});

describe("how far in a row sits", () => {
	test("the depth reaches the stylesheet as a custom property", () => {
		expect(indent(3)).toEqual({ "--depth": 3 });
	});
});
