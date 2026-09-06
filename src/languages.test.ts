import { describe, expect, test } from "vitest";
import { named, offered } from "./languages";

describe("offered", () => {
	test("puts the computer's own language first", () => {
		expect(offered("fr")[0]).toEqual({ tag: "fr", name: "French" });
	});

	test("reads a region off a locale it knows the language of", () => {
		expect(offered("en-GB")[0].tag).toBe("en");
	});

	test("prefers a whole tag to the language in front of it", () => {
		expect(offered("pt-BR")[0].name).toBe("Portuguese (Brazil)");
	});

	test("moves a language rather than repeating it", () => {
		const all = offered("ja");
		const japanese = all.filter((language) => language.tag === "ja");

		expect(japanese).toHaveLength(1);
		expect(all).toHaveLength(offered("").length);
	});

	test("offers a language it has never heard of", () => {
		const all = offered("cy");

		expect(all[0].tag).toBe("cy");
		expect(all).toHaveLength(offered("").length + 1);
	});

	test("leaves the order alone when nothing is asked for", () => {
		expect(offered("")[0].tag).toBe("en");
	});
});

describe("named", () => {
	test("names a tag it knows", () => {
		expect(named("de")).toBe("German");
	});

	test("names a regional tag by its language", () => {
		expect(named("es-MX")).toBe("Spanish");
	});

	test("says nothing about nothing", () => {
		expect(named("")).toBe("");
	});
});
