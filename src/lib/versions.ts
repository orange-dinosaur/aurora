// How the Versions tab words a row and groups the list. Checkpoints arrive
// newest first, so grouping only has to notice where one day gives way to the
// next.

import { timeOfDay, when } from "../dates";
import type {
	Change,
	Changes,
	Checkpoint,
	Merging,
	Status,
	StatusRow,
} from "../types";

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
	} else if (change.takes) {
		parts.push("takes changed");
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

/** What a put back writes over. The book puts back the cover with it. */
export type Overwrites =
	| { kind: "everything" }
	| { kind: "document"; id: string }
	| { kind: "book" };

/**
 * The names of what a put back would write over that has changes not in a
 * checkpoint. A folder holds no writing of its own, so it is never named.
 */
export function overwritten(status: Status, target: Overwrites): string[] {
	const names = new Map<string, string>();
	for (const line of [...lines(status.staged), ...lines(status.unstaged)]) {
		const change = line.change;
		const hit =
			target.kind === "everything"
				? change?.folder !== true
				: target.kind === "book"
					? change === null
					: change?.id === target.id;
		if (hit) {
			names.set(
				lineKey(line),
				line.name ??
					(line.row.kind === "book" ? "The book" : "The cover"),
			);
		}
	}
	return [...names.values()];
}

/** The question before a put back that would write over those names. */
export function overwriteQuestion(names: string[]): string {
	const who =
		names.length === 1
			? `${names[0]} has`
			: `${names.length.toLocaleString()} documents have`;
	return `${who} changes that aren't in a checkpoint. Putting back overwrites them.`;
}

/** A merge waiting to be settled, and the name of the version it goes into. */
export type MergeUnderway = { merging: Merging; into: string };

/** A row of the Changes list that a merge left to settle. */
export type SettleLine = {
	key: string;
	name: string;
	what: string;
	/** A conflicted document, or the one row for the binder and details. */
	kind: "document" | "binder";
	/** The document's id, null for the binder and for a file no manifest names. */
	id: string | null;
	/** Where the document sits, null for the binder. */
	path: string | null;
};

/**
 * The rows a merge leaves to settle: each conflicted document with its count
 * of spots, then one row for every conflict outside the text. A document
 * whose spots are not counted yet shows none.
 */
export function settleLines(
	merging: Merging,
	spots: Record<string, number>,
): SettleLine[] {
	const documents = merging.conflicts.flatMap((conflict): SettleLine[] => {
		if (conflict.kind !== "text") {
			return [];
		}
		const count = conflict.id === null ? undefined : spots[conflict.id];
		return [
			{
				key: conflict.path,
				name: nameIn(conflict.path),
				what:
					count === undefined
						? ""
						: `${count.toLocaleString()} ${count === 1 ? "spot" : "spots"}`,
				kind: "document",
				id: conflict.id,
				path: conflict.path,
			},
		];
	});
	const rest = merging.conflicts.length - documents.length;
	if (rest === 0) {
		return documents;
	}
	return [
		...documents,
		{
			key: "binder",
			name: "Binder and details",
			what: `${rest.toLocaleString()} to settle`,
			kind: "binder",
			id: null,
			path: null,
		},
	];
}

/** The banner's line under "Merging X into Y". */
export function settleBanner(rows: number): string {
	if (rows === 0) {
		return "Everything is settled. Add the checkpoint to finish the merge.";
	}
	const things = rows === 1 ? "1 thing" : `${rows.toLocaleString()} things`;
	return `${things} to settle before the merge can be a checkpoint.`;
}

/** Why Add checkpoint is off while a merge has rows to settle. */
export function settleFirst(rows: number): string {
	return rows === 1
		? "Settle the marked row first."
		: `Settle the ${rows.toLocaleString()} marked rows first.`;
}
