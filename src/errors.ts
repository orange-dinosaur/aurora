// What a rejected `invoke` carries. Every Rust command fails with
// `project::Error`, which crosses as a kind to switch on and a message to show.

export type CommandError = {
	kind: string;
	message: string;
};

/**
 * Reads a rejection back. Nothing guarantees the shape: a panic inside a
 * command, or the bridge itself failing, arrives as something else entirely.
 */
export function failure(error: unknown): CommandError {
	if (
		typeof error === "object" &&
		error !== null &&
		"kind" in error &&
		"message" in error &&
		typeof error.kind === "string" &&
		typeof error.message === "string"
	) {
		return { kind: error.kind, message: error.message };
	}

	return { kind: "unknown", message: String(error) };
}
