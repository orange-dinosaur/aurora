import { describe, expect, test } from "vitest";
import { folderName, going } from "./recents";

describe("going", () => {
	test("counts the documents when there are several", () => {
		expect(going(14)).toBe("14 documents go to the system trash.");
	});

	test("does not say 1 documents", () => {
		expect(going(1)).toBe("One document goes to the system trash.");
	});

	test("stays vague when the manifest could not be read", () => {
		expect(going(null)).toBe("The folder goes to the system trash.");
	});

	test("stays vague rather than offering 0 documents", () => {
		expect(going(0)).toBe("The folder goes to the system trash.");
	});

	test("groups a count large enough to need it", () => {
		expect(going(1240)).toBe("1,240 documents go to the system trash.");
	});
});

describe("folderName", () => {
	test("takes the last part of a Unix path", () => {
		expect(folderName("/home/ines/old-draft-2019")).toBe("old-draft-2019");
	});

	test("takes the last part of a Windows path", () => {
		expect(folderName("C:\\Users\\ines\\old-draft-2019")).toBe(
			"old-draft-2019",
		);
	});

	test("ignores a trailing separator", () => {
		expect(folderName("/home/ines/old-draft-2019/")).toBe("old-draft-2019");
	});
});
