import { describe, expect, test } from "vitest";
import { trashed } from "./trash";

describe("what a folder in the trash took with it", () => {
	test("an empty folder still says it is one", () => {
		expect(trashed(0)).toBe("an empty folder");
	});

	test("one document is not one documents", () => {
		expect(trashed(1)).toBe("a folder of 1 document");
	});

	test("a chapter of fourteen scenes counts every one of them", () => {
		expect(trashed(14)).toBe("a folder of 14 documents");
	});
});
