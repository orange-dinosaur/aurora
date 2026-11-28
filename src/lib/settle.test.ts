import { describe, expect, it } from "vitest";
import { shift } from "./settle";

describe("shift", () => {
	it("pushes a thing back to where it stood", () => {
		expect(shift({ left: 10, top: 40 }, { left: 130, top: 40 })).toEqual({
			x: -120,
			y: 0,
		});
	});

	it("leaves a thing that stayed alone", () => {
		expect(shift({ left: 10, top: 40 }, { left: 10, top: 40 })).toBeNull();
	});

	it("ignores less than half a pixel", () => {
		expect(
			shift({ left: 10.2, top: 40 }, { left: 10, top: 40.3 }),
		).toBeNull();
	});
});
