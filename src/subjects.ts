// Which documents are pages about something rather than the story itself. The
// rule is where a document sits, not what the project was created with, so a
// writer who deletes the seeded folder and makes their own `Characters` gets
// the same behaviour.

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
