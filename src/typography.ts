// The three numbers that decide how the page reads, and how far one press
// moves each of them. Kept apart from the popover so the arithmetic can be
// tested without a browser.

import type { ManuscriptFont, Preferences } from "./types";

/** A preference the writer nudges rather than types. */
export type Setting = {
	id: "measure" | "fontSize" | "lineHeight";
	/** In the popover, where one word has to do. */
	label: string;
	/** In Settings, which has room for what the thing is actually called. */
	full: string;
	/** A second line under that name, where the name alone is not enough. */
	hint?: string;
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
		full: "Measure",
		hint: "Characters per line",
		min: 40,
		max: 100,
		step: 2,
		show: (value) => `${value}`,
	},
	{
		id: "fontSize",
		label: "Size",
		full: "Size",
		min: 12,
		max: 24,
		step: 1,
		show: (value) => `${value}px`,
	},
	{
		id: "lineHeight",
		label: "Spacing",
		full: "Line spacing",
		min: 1.2,
		max: 2.4,
		step: 0.1,
		show: (value) => value.toFixed(1),
	},
];

/** What each face falls back through while its file is still loading, or if it
 * ever stops being vendored. Keyed rather than searched, so a face that is
 * added cannot be left without a stack. */
const STACKS: Record<ManuscriptFont, string> = {
	spectral: "Spectral, Georgia, serif",
	newsreader: "Newsreader, Georgia, serif",
	sourceSerif: '"Source Serif 4", Georgia, serif',
	plexMono: '"IBM Plex Mono", ui-monospace, monospace',
	plexSans: '"IBM Plex Sans", system-ui, sans-serif',
	system: "system-ui, sans-serif",
};

/** The faces in the order the picker offers them: the three serifs first,
 * since this is a manuscript, then the typewriter and the two sans. */
export const FACES: { id: ManuscriptFont; family: string }[] = [
	{ id: "spectral", family: "Spectral" },
	{ id: "newsreader", family: "Newsreader" },
	{ id: "sourceSerif", family: "Source Serif 4" },
	{ id: "plexMono", family: "Plex Mono · typewriter" },
	{ id: "plexSans", family: "Plex Sans" },
	{ id: "system", family: "System UI" },
];

/** What to set `--serif` to for the face the writer chose. */
export function face(font: ManuscriptFont): string {
	return STACKS[font];
}

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
