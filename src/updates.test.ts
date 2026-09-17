import { describe, expect, test } from "vitest";
import {
	HIDDEN,
	action,
	closable,
	message,
	offered,
	pretendFails,
	pretended,
	starting,
	stumbled,
} from "./updates";

describe("the update notice's states", () => {
	test("an update that is found is offered with its version", () => {
		expect(offered("0.1.1")).toEqual({
			kind: "available",
			version: "0.1.1",
		});
	});

	test("starting keeps the version it was offered", () => {
		expect(starting(offered("0.1.1"))).toEqual({
			kind: "updating",
			version: "0.1.1",
		});
	});

	test("nothing starts from hidden", () => {
		expect(starting(HIDDEN)).toEqual(HIDDEN);
	});

	test("a failure can be started again", () => {
		const failed = stumbled(starting(offered("0.1.1")));

		expect(starting(failed)).toEqual({
			kind: "updating",
			version: "0.1.1",
		});
	});

	test("only an update in progress can fail", () => {
		expect(stumbled(offered("0.1.1"))).toEqual(offered("0.1.1"));
	});

	test("the preview stops where the restart would be", () => {
		expect(pretended(starting(offered("0.9.9")))).toEqual({
			kind: "pretended",
			version: "0.9.9",
		});
	});

	test("the preview fails on purpose for a version ending in .0", () => {
		expect(pretendFails("0.9.0")).toBe(true);
		expect(pretendFails("0.9.9")).toBe(false);
	});
});

describe("the update notice's wording", () => {
	test("an available update names itself", () => {
		expect(message(offered("0.1.1"))).toBe("Aurora 0.1.1 is ready");
		expect(action(offered("0.1.1"))).toBe("Update and restart");
	});

	test("an update in progress says so and offers nothing", () => {
		const updating = starting(offered("0.1.1"));

		expect(message(updating)).toBe("Updating…");
		expect(action(updating)).toBeNull();
	});

	test("a failure offers another go", () => {
		const failed = stumbled(starting(offered("0.1.1")));

		expect(message(failed)).toBe("The update didn't finish.");
		expect(action(failed)).toBe("Try again");
	});

	test("a hidden notice says nothing", () => {
		expect(message(HIDDEN)).toBe("");
		expect(action(HIDDEN)).toBeNull();
	});
});

describe("putting the notice away", () => {
	test("an offer and a failure can both be closed", () => {
		expect(closable(offered("0.1.1"))).toBe(true);
		expect(closable(stumbled(starting(offered("0.1.1"))))).toBe(true);
	});

	test("an update in progress cannot", () => {
		expect(closable(starting(offered("0.1.1")))).toBe(false);
	});
});
