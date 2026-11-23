import { describe, expect, test } from "vitest";
import { abbreviated, deliberate, indent, retitling } from "./rows";

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

describe("a folder row's count", () => {
	test("a short folder says exactly what it holds", () => {
		expect(abbreviated(1)).toBe("1");
		expect(abbreviated(999)).toBe("999");
	});

	test("an empty one shows nothing", () => {
		expect(abbreviated(0)).toBe("");
	});

	test("a longer one is cut down to thousands", () => {
		expect(abbreviated(1200)).toBe("1.2k");
		expect(abbreviated(48_300)).toBe("48k");
	});

	test("what is cut is never rounded up", () => {
		expect(abbreviated(1999)).toBe("1.9k");
		expect(abbreviated(9999)).toBe("9.9k");
	});
});

describe("deliberate", () => {
	test("a single click and the keyboard count", () => {
		expect(deliberate(1)).toBe(true);
		expect(deliberate(0)).toBe(true);
	});

	test("the second click of a double click does not", () => {
		expect(deliberate(2)).toBe(false);
	});
});
