// The wording the Shelf uses about a project. Here rather than in the screen
// because each of these has more cases than it looks like it has, and each of
// them is worth pinning down.

/**
 * What goes when a project is deleted, as the line under the question.
 *
 * A project whose manifest could not be read has no count to give, and one
 * whose documents have all been deleted would otherwise be offered as "0
 * documents". Both say the vaguer thing, which is still true.
 */
export function going(documents: number | null): string {
	if (documents === null || documents === 0) {
		return "The folder goes to the system trash.";
	}
	if (documents === 1) {
		return "One document goes to the system trash.";
	}
	return `${documents.toLocaleString()} documents go to the system trash.`;
}

/** The last part of a project's path, on either kind of system. */
export function folderName(root: string): string {
	return root.split(/[\\/]/).filter(Boolean).pop() ?? root;
}
