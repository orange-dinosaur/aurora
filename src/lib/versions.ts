// How the Versions tab words a row and groups the list. Versions arrive newest
// first, so grouping only has to notice where one day gives way to the next.

import { timeOfDay, when } from "../dates";
import type { Version } from "../types";

/** A day of versions, with the heading it is listed under. */
export type VersionDay = {
	heading: string;
	versions: Version[];
};

/** The list broken into the writer's own days, keeping its order. */
export function byDay(versions: Version[], now: Date): VersionDay[] {
	const today = now.toDateString();
	const before = new Date(now);
	before.setDate(before.getDate() - 1);
	const yesterday = before.toDateString();

	const days: (VersionDay & { key: string })[] = [];
	for (const version of versions) {
		const key = new Date(version.at).toDateString();
		const open = days[days.length - 1];
		if (open !== undefined && open.key === key) {
			open.versions.push(version);
			continue;
		}
		const heading =
			key === today
				? "Today"
				: key === yesterday
					? "Yesterday"
					: when(version.at);
		days.push({ key, heading, versions: [version] });
	}
	return days.map(({ heading, versions }) => ({ heading, versions }));
}

/** A session's length: "42 min" under an hour, "1 h 04" from there. */
export function duration(minutes: number): string {
	if (minutes < 60) {
		return `${minutes} min`;
	}
	const hours = Math.floor(minutes / 60);
	return `${hours} h ${String(minutes % 60).padStart(2, "0")}`;
}

/** Words written and removed, leaving out whichever is nought. */
export function counts(written: number, removed: number): string {
	return [
		written > 0 ? `+${written.toLocaleString()}` : "",
		removed > 0 ? `−${removed.toLocaleString()}` : "",
	]
		.filter((part) => part !== "")
		.join(" ");
}

/**
 * The quiet line under a named version, or the whole of an automatic one:
 * "15:40 · session, 42 min · +610".
 */
export function detail(version: Version): string {
	const parts = [timeOfDay(version.at)];
	const moved = counts(version.written ?? 0, version.removed ?? 0);

	switch (version.kind) {
		case "session":
			parts.push(`session, ${duration(version.minutes ?? 0)}`, moved);
			break;
		case "closing":
			parts.push("on leaving", moved);
			break;
		case "named":
		case "beforePuttingBack":
			if (version.words !== null) {
				parts.push(`${version.words.toLocaleString()} words`);
			}
			parts.push("you");
			break;
		case null:
			// A commit made outside Aurora, by whoever made it.
			parts.push(version.author);
			break;
	}
	return parts.filter((part) => part !== "").join(" · ");
}
