// How the Versions tab words a row and groups the list. Checkpoints arrive
// newest first, so grouping only has to notice where one day gives way to the
// next.

import { timeOfDay, when } from "../dates";
import type { Change, Changes, Checkpoint, StatusRow } from "../types";

/** A day of checkpoints, with the heading it is listed under. */
export type CheckpointDay = {
	heading: string;
	checkpoints: Checkpoint[];
};

/** The list broken into the writer's own days, keeping its order. */
export function byDay(checkpoints: Checkpoint[], now: Date): CheckpointDay[] {
	const today = now.toDateString();
	const before = new Date(now);
	before.setDate(before.getDate() - 1);
	const yesterday = before.toDateString();

	const days: (CheckpointDay & { key: string })[] = [];
	for (const checkpoint of checkpoints) {
		const key = new Date(checkpoint.at).toDateString();
		const open = days[days.length - 1];
		if (open !== undefined && open.key === key) {
			open.checkpoints.push(checkpoint);
			continue;
		}
		const heading =
			key === today
				? "Today"
				: key === yesterday
					? "Yesterday"
					: when(checkpoint.at);
		days.push({ key, heading, checkpoints: [checkpoint] });
	}
	return days.map(({ heading, checkpoints }) => ({ heading, checkpoints }));
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
 * The quiet line under a checkpoint's message: "15:40 · Ines", with the
 * document's words when the list is of one document.
 */
export function detail(checkpoint: Checkpoint): string {
	return [
		timeOfDay(checkpoint.at),
		checkpoint.author,
		checkpoint.words === null
			? ""
			: `${checkpoint.words.toLocaleString()} words`,
	]
		.filter((part) => part !== "")
		.join(" · ");
}

/**
 * What a checkpoint is called in the one kept before putting it back: the
 * first line of its message.
 */
export function label(checkpoint: Checkpoint): string {
	return checkpoint.message.split("\n")[0].trim();
}

function nameIn(path: string): string {
	return path.split("/").slice(-1)[0].replace(/\.md$/, "");
}

/**
 * A changed document or folder as a row: its name, and what happened to it,
 * such as "Renamed from “Old”, moved to Chapter 3, edited".
 */
export function summary(change: Change): { name: string; what: string } {
	const kind = change.folder ? "folder" : "document";
	if (change.after === null) {
		return { name: nameIn(change.before ?? ""), what: `Deleted ${kind}` };
	}
	const name = nameIn(change.after);
	if (change.before === null) {
		return { name, what: `New ${kind}` };
	}

	const parts: string[] = [];
	if (change.renamed) {
		parts.push(`renamed from “${nameIn(change.before)}”`);
	}
	if (change.moved) {
		const parent = change.after.split("/").slice(-2, -1)[0];
		parts.push(
			parent === undefined ? "moved to the top" : `moved to ${parent}`,
		);
	}
	if (change.reordered.length > 0) {
		const moves = change.reordered.map(
			({ name, after }) =>
				`“${nameIn(name)}” moved ${after === null ? "to the top" : `after “${nameIn(after)}”`}`,
		);
		parts.push(`order changed: ${moves.join(", ")}`);
	}
	if (change.edited) {
		parts.push("edited");
	}
	const what = parts.join(", ");
	return { name, what: what.charAt(0).toUpperCase() + what.slice(1) };
}

/** A row of the Changes list. The change is null on the book and the cover. */
export type Line = {
	row: StatusRow;
	name?: string;
	what: string;
	change: Change | null;
};

/** One group of the Changes list as rows: the book and cover, then the rest. */
export function lines(changes: Changes): Line[] {
	const book: Line[] = [];
	if (changes.book) {
		book.push({
			row: { kind: "book" },
			what: "Book details changed",
			change: null,
		});
	}
	if (changes.cover) {
		book.push({
			row: { kind: "cover" },
			what: "Cover changed",
			change: null,
		});
	}
	return [
		...book,
		...changes.nodes.map((change): Line => ({
			row: { kind: "node", id: change.id },
			...summary(change),
			change,
		})),
	];
}

/** What tells one row from the others in its group. */
export function lineKey(line: Line): string {
	return line.row.kind === "node" ? line.row.id : line.row.kind;
}
