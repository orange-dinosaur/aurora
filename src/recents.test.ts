import { describe, expect, test } from "vitest";
import { folderName, going, place, tilde } from "./recents";

describe("tilde", () => {
	test("shortens a folder under home", () => {
		expect(tilde("/home/ines/Documents/Aurora", "/home/ines")).toBe(
			"~/Documents/Aurora",
		);
	});

	test("leaves the folder whole while home is unknown", () => {
		expect(tilde("/home/ines/Documents", null)).toBe(
			"/home/ines/Documents",
		);
	});
});

describe("place", () => {
	test("writes the home folder as ~", () => {
		expect(place("/home/ines/Writing/Ithaca", "/home/ines")).toBe(
			"~/Writing",
		);
	});

	test("is just ~ for a project straight in the home folder", () => {
		expect(place("/home/ines/Ithaca", "/home/ines/")).toBe("~");
	});

	test("leaves a folder outside home alone", () => {
		expect(place("/mnt/books/Ithaca", "/home/ines")).toBe("/mnt/books");
	});

	test("does not take a sibling of home for home", () => {
		expect(place("/home/ines2/Ithaca", "/home/ines")).toBe("/home/ines2");
	});

	test("shortens a Windows path", () => {
		expect(place("C:\\Users\\ines\\Books\\Ithaca", "C:\\Users\\ines")).toBe(
			"~\\Books",
		);
	});

	test("ignores a trailing separator on the project", () => {
		expect(place("/mnt/books/Ithaca/", null)).toBe("/mnt/books");
	});

	test("gives the root for a project at the top of the disk", () => {
		expect(place("/Ithaca", null)).toBe("/");
	});
});

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
