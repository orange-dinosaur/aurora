// Recognising the people and places a document talks about. A subject is a
// document in its own right — a character page, a place — and the names it
// answers to are its title plus whatever the writer put in its `names` field.
// This finds where those names appear in someone else's prose.
//
// Nothing here touches a node or reads a file. It works on the runs the editor
// hands over, the same copy Find and project-wide search work on, so it can
// never dirty a document.

import { type Match, type Run, matches } from "./find";

/** A document the writer keeps a page about, and what it answers to. */
export type Subject = {
	/** The document's path, which is how a hit is traced back to its page. */
	path: string;
	/** The page's title, which always counts as one of its names. */
	title: string;
	/** The other names, from the `names` field. */
	names: string[];
};

/** One place a subject was named. */
export type Mention = {
	/** The `path` of the subject that was recognised. */
	subject: string;
	/** Which of its names appeared here, as it was written. */
	name: string;
	at: Match;
};

/**
 * Every name a subject answers to, longest first, with blanks and repeats
 * dropped. The title leads because the writer never has to type it twice.
 */
function namesOf(subject: Subject): string[] {
	const all = [subject.title, ...subject.names]
		.map((name) => name.trim())
		.filter((name) => name !== "");

	return [...new Set(all)].sort((one, other) => other.length - one.length);
}

/**
 * Every occurrence of every subject's names in `runs`, in reading order.
 *
 * Recognition is whole-word and case-sensitive, which is what tells `Rose`
 * from `rose`, `Rosemary` and `prose`. Two subjects that share a name are both
 * reported at the same place; deciding between them is not this module's job.
 */
export function mentions(subjects: Subject[], runs: Run[]): Mention[] {
	const found: Mention[] = [];

	for (const subject of subjects) {
		for (const name of namesOf(subject)) {
			for (const at of matches(runs, name, {
				wholeWord: true,
				caseSensitive: true,
			})) {
				found.push({ subject: subject.path, name, at });
			}
		}
	}

	// Each subject's hits come out in reading order but the subjects were
	// walked one after another, so the whole list has to be put back in the
	// order a reader would meet them.
	const order = new Map(runs.map((run, index) => [run.key, index]));

	return found.sort((one, other) => {
		const run =
			(order.get(one.at.fromKey) ?? 0) -
			(order.get(other.at.fromKey) ?? 0);
		return run !== 0 ? run : one.at.fromOffset - other.at.fromOffset;
	});
}
