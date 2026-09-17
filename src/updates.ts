/**
 * What the update notice is doing. Nothing here talks to the updater plugin:
 * the component drives the plugin and moves the notice through these.
 */
export type Notice =
	| { kind: "hidden" }
	| { kind: "available"; version: string }
	| { kind: "updating"; version: string }
	| { kind: "failed"; version: string }
	/** Only reachable from the development preview, which never restarts. */
	| { kind: "pretended"; version: string };

export const HIDDEN: Notice = { kind: "hidden" };

/** An update was found. */
export function offered(version: string): Notice {
	return { kind: "available", version };
}

/** The writer pressed the button. Only a notice with a version can start. */
export function starting(notice: Notice): Notice {
	return notice.kind === "available" || notice.kind === "failed"
		? { kind: "updating", version: notice.version }
		: notice;
}

/** The download or the install gave up. */
export function stumbled(notice: Notice): Notice {
	return notice.kind === "updating"
		? { kind: "failed", version: notice.version }
		: notice;
}

/** Where the development preview stops, in place of the restart. */
export function pretended(notice: Notice): Notice {
	return notice.kind === "updating"
		? { kind: "pretended", version: notice.version }
		: notice;
}

/**
 * The preview fails on purpose for a version ending in `.0`, so that the
 * failed state can be looked at without anything actually going wrong.
 */
export function pretendFails(version: string): boolean {
	return version.endsWith(".0");
}

/** What the notice reads. Empty when there is nothing to show. */
export function message(notice: Notice): string {
	switch (notice.kind) {
		case "available":
			return `Aurora ${notice.version} is ready`;
		case "updating":
			return "Updating…";
		case "failed":
			return "The update didn't finish.";
		case "pretended":
			return `Aurora would restart into ${notice.version} now.`;
		default:
			return "";
	}
}

/** The button beside it, or null when there is nothing to press. */
export function action(notice: Notice): string | null {
	switch (notice.kind) {
		case "available":
			return "Update and restart";
		case "failed":
			return "Try again";
		default:
			return null;
	}
}

/** Whether the writer can put the notice away right now. */
export function closable(notice: Notice): boolean {
	return notice.kind !== "hidden" && notice.kind !== "updating";
}
