// Which way the header's theme button moves. The preference has three states
// and the button has two, so what it does depends on what is actually drawn
// rather than only on what is stored.

import type { Theme } from "./types";

/** The theme on screen now, which is the desktop's when nothing overrides it. */
export function showing(theme: Theme, systemIsDark: boolean): "light" | "dark" {
	if (theme === "system") {
		return systemIsDark ? "dark" : "light";
	}

	return theme;
}

/**
 * What the button switches to.
 *
 * Following the desktop is not one of the two states it moves between: pressing
 * it always leaves the preference explicit, and Settings is where the writer
 * goes back to System. Otherwise a writer on a dark desktop would press it once
 * and get the same dark they already had.
 */
export function flipped(theme: Theme, systemIsDark: boolean): "light" | "dark" {
	return showing(theme, systemIsDark) === "dark" ? "light" : "dark";
}
