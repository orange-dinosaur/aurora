// The three numbers that decide how the page reads, and how far one press
// moves each of them. Kept apart from the popover so the arithmetic can be
// tested without a browser.

import type { Preferences } from "./types";

/** A preference the writer nudges rather than types. */
export type Setting = {
	id: "measure" | "fontSize" | "lineHeight";
	label: string;
	min: number;
	max: number;
	step: number;
	/** The value as the popover shows it, with its unit. */
	show: (value: number) => string;
};

// The bounds are what stays readable, not what CSS would allow.
export const SETTINGS: Setting[] = [
	{
		id: "measure",
		label: "Width",
		min: 40,
		max: 100,
		step: 2,
		show: (value) => `${value}`,
	},
	{
		id: "fontSize",
		label: "Size",
		min: 12,
		max: 24,
		step: 1,
		show: (value) => `${value}px`,
	},
	{
		id: "lineHeight",
		label: "Spacing",
		min: 1.2,
		max: 2.4,
		step: 0.1,
		show: (value) => value.toFixed(1),
	},
];

/** One press, up or down, kept inside the setting's range. Rounded because a
 * tenth cannot be added to a float without drift — 1.7 + 0.1 is 1.7999… — and
 * that drift would be written to the store and shown to the writer. */
export function moved(
	setting: Setting,
	value: number,
	presses: number,
): number {
	const next = Number((value + presses * setting.step).toFixed(2));
	return Math.min(setting.max, Math.max(setting.min, next));
}

/** The same preferences with one setting changed. */
export function nudged(
	preferences: Preferences,
	setting: Setting,
	presses: number,
): Preferences {
	const next = { ...preferences };
	next[setting.id] = moved(setting, preferences[setting.id], presses);
	return next;
}
