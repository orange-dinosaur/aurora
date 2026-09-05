// The wording the welcome screen uses about a project it is about to move to
// the trash. Here rather than in the screen because the count has more cases
// than it looks like it has, and each of them is worth pinning down.

/**
 * What to call the writing inside a project in the question asked before it is
 * trashed, as the tail of "Move <name> …to the system trash?".
 *
 * A project whose manifest could not be read has no count to give, and one
 * whose documents have all been deleted would otherwise be offered as "all 0
 * of its documents". Both say the vaguer thing, which is still true.
 */
export function contents(documents: number | null): string {
	if (documents === null || documents === 0) {
		return "and everything in it";
	}
	if (documents === 1) {
		return "and its one document";
	}
	return `and all ${documents.toLocaleString()} of its documents`;
}
