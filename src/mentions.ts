// Recognising the people and places a document talks about. A subject is a
// document in its own right — a character page, a place — and the names it
// answers to are its title plus whatever the writer put in its `names` field.
// This finds where those names appear in someone else's prose.
//
// Nothing here touches a node or reads a file. It works on the runs the editor
// hands over, the same copy Find and project-wide search work on, so it can
// never dirty a document.

import { type Match, type Run, matches } from "./find";
import { lineOf, locate, type Searchable, type Unreadable } from "./search";

/** A document the writer keeps a page about, and what it answers to. */
export type Subject = {
	/** The document's id, which is how a hit is traced back to its page. */
	id: string;
	/** The page's title, which always counts as one of its names. */
	title: string;
	/** The other names, from the `names` field. */
	names: string[];
};

/** One place a subject was named. */
export type Mention = {
	/** The `id` of the subject that was recognised. */
	subject: string;
	/** Which of its names appeared here, as it was written. */
	name: string;
	at: Match;
};

/** How recognition reads a name, as against how a typed query is read. */
export const RECOGNITION = { wholeWord: true, caseSensitive: true };

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
			for (const at of matches(runs, name, RECOGNITION)) {
				found.push({ subject: subject.id, name, at });
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

/** A document as recognition reads it: search's view of it, plus its tags. */
export type Mentionable = Searchable & {
	/** The `tags` field of its front matter, or empty when it has none. */
	tags: string[];
};

/** One place in a document where the subject was named. */
export type MentionHit = {
	/** Which of the subject's names appeared here. */
	name: string;
	/**
	 * Which occurrence of that name this is within the document, counting from
	 * zero. Counted per name rather than per document, because it is what Find
	 * is seeded with and Find steps through one name at a time.
	 */
	ordinal: number;
	/** The text of the block the name sits in. */
	line: string;
	/** Where the name starts and ends within `line`. */
	from: number;
	to: number;
};

/** Everything one document has to say about the subject. */
export type MentionGroup = {
	id: string;
	title: string;
	trail: string[];
	/**
	 * The subject's names this document carries as tags. A tag is a claim the
	 * writer made about the whole document, so it has no position in the text
	 * and opens without moving the caret, the way a title hit does.
	 */
	tagged: string[];
	hits: MentionHit[];
	/** Its tags and its hits together, which is what the header counts. */
	count: number;
};

export type MentionResults = {
	/** Only documents that say something, in the order they were given. */
	groups: MentionGroup[];
	unreadable: Unreadable[];
};

/**
 * Where in the project the subject is named or tagged.
 *
 * Its own page is skipped: a character page is full of her name and saying so
 * on the page itself would be no use. Tags are matched against the same names
 * and with the same case sensitivity as the prose, so a document tagged `elena`
 * is not Elena's until step 12 decides otherwise.
 */
export function mentionsIn(
	subject: Subject,
	documents: Mentionable[],
): MentionResults {
	const names = namesOf(subject);
	const groups: MentionGroup[] = [];
	const unreadable: Unreadable[] = [];

	for (const { id, title, trail, tags, runs } of documents) {
		if (id === subject.id) {
			continue;
		}
		if (runs === null) {
			unreadable.push({ id, title, trail });
			continue;
		}

		const tagged = names.filter((name) => tags.includes(name));
		const located = locate(runs);
		// Every name's occurrences are numbered on their own, so the count has
		// to be carried across the walk rather than taken from the hit's place
		// in the list.
		const seen = new Map<string, number>();
		const hits: MentionHit[] = [];

		for (const { name, at } of mentions([subject], runs)) {
			const where = lineOf(located, at);
			if (where === null) {
				continue;
			}

			const ordinal = seen.get(name) ?? 0;
			seen.set(name, ordinal + 1);
			hits.push({ name, ordinal, ...where });
		}

		if (tagged.length === 0 && hits.length === 0) {
			continue;
		}

		groups.push({
			id,
			title,
			trail,
			tagged,
			hits,
			count: hits.length + tagged.length,
		});
	}

	return { groups, unreadable };
}
