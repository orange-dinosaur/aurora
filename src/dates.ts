// Dates cross from Rust as RFC 3339 strings. This is the one place that turns
// one into something to read, so a document's card and a trash entry never
// disagree about how a date looks.

/**
 * The time of day on its own, for a list that already carries the date on the
 * heading above it.
 */
export function timeOfDay(iso: string): string {
	const at = new Date(iso);
	return Number.isNaN(at.getTime())
		? iso
		: at.toLocaleTimeString(undefined, {
				hour: "numeric",
				minute: "2-digit",
			});
}

/** A date as the writer reads it, or the raw string if it is not one. */
export function when(iso: string): string {
	const at = new Date(iso);
	return Number.isNaN(at.getTime())
		? iso
		: at.toLocaleDateString(undefined, {
				day: "numeric",
				month: "short",
				year: "numeric",
			});
}
