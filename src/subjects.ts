// Which documents are pages about something rather than the story itself. The
// rule is where a document sits, not what the project was created with, so a
// writer who deletes the seeded folder and makes their own `Characters` gets
// the same behaviour.

import { custom, text, type Fields, type Ties } from "./frontmatter";
import type { TreeNode } from "./types";

/** The sections whose documents are subjects. */
export const SUBJECT_SECTIONS = ["Characters", "Locations"];

/**
 * Whether a document is a subject: a page about a person or a place, which
 * answers to names of its own and collects the places it is mentioned.
 *
 * `trail` is the folders it sits in, from its section down, so a document
 * anywhere under `Characters/` counts however deep it is filed. Notes and
 * Outline are left out because their documents get generic titles that turn up
 * in ordinary sentences, and the Manuscript because a chapter called "The
 * Return" would match that phrase anywhere.
 */
export function isSubject(trail: string[]): boolean {
	return trail.length > 0 && SUBJECT_SECTIONS.includes(trail[0]);
}

/**
 * Every subject in the project by title, in the order the tree lists them, so
 * a relationship can be offered the pages it could point at. Two subjects of
 * the same title are one suggestion: the name is all a tie records.
 */
export function subjectNames(nodes: TreeNode[]): string[] {
	const found: string[] = [];

	function walk(node: TreeNode) {
		if (node.node === "folder") {
			node.children.forEach(walk);
			return;
		}
		if (isSubject(node.trail) && !found.includes(node.title)) {
			found.push(node.title);
		}
	}

	nodes.forEach(walk);
	return found;
}

/**
 * The field a subject's page reads as its subtitle, by section. A character is
 * introduced by what they are to the story and a place by where it is, and both
 * are ordinary fields the writer typed rather than anything Aurora keeps.
 */
const LEAD: Record<string, string | undefined> = {
	Characters: "role",
	Locations: "region",
};

/** One of the small boxes across the top of a subject's page. */
export type Box = { label: string; value: string };

/** What a subject's page says about it, above the notes and the appearances. */
export type Facts = { subtitle: string; boxes: Box[] };

/** Shown in a box the writer has left empty, so the box keeps its height. */
const NOTHING = "—";

/**
 * A subject's page as facts rather than as markup, so the arrangement can be
 * tested without rendering anything.
 *
 * The boxes are the fields the writer gave the page, in the order the file
 * lists them, with the subtitle's own field taken out of the row and the
 * counted ones added after. A location is the only one that says where it is
 * first met: a character is met wherever they are named, and the appearances
 * below already say where that is.
 */
export function factsOf(
	trail: string[],
	fields: Fields,
	counted: { mentions: number; first: string | null },
): Facts {
	const section = trail[0] ?? "";
	const lead = LEAD[section];

	const boxes: Box[] = custom(fields)
		.filter((key) => key !== lead)
		.map((key) => ({
			label: key,
			value: text(fields, key) === "" ? NOTHING : text(fields, key),
		}));

	if (section === "Locations") {
		boxes.push({
			label: "First appears",
			value: counted.first ?? NOTHING,
		});
	}

	boxes.push({ label: "Mentions", value: String(counted.mentions) });

	return {
		subtitle: lead === undefined ? "" : text(fields, lead),
		boxes,
	};
}

/** One relationship as the page draws it. */
export type Link = {
	/** The name the writer wrote, which is what the button says. */
	name: string;
	/** What they said the tie is, which may be nothing yet. */
	note: string;
	/**
	 * The page to go to, or null when nothing in the project answers to the
	 * name. A tie may be written before its subject exists, and saying so is
	 * better than a button that does nothing.
	 */
	id: string | null;
};

/** The least a subject has to be for a relationship to find it. */
type Named = { id: string; title: string; names: string[] };

/**
 * A subject's relationships paired with the pages they point at, in the order
 * the file lists them.
 *
 * A tie is written against a title, since that is what the picker offers, but
 * the other names a page answers to are tried as well: a writer who renamed a
 * character and left the old name in `names` should not lose the tie.
 */
export function linksOf(ties: Ties, subjects: Named[]): Link[] {
	return [...ties].map(([name, note]) => {
		const found =
			subjects.find((subject) => subject.title === name) ??
			subjects.find((subject) => subject.names.includes(name));

		return { name, note, id: found?.id ?? null };
	});
}
