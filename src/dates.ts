// Dates cross from Rust as RFC 3339 strings. This is the one place that turns
// one into something to read, so a document's card and a trash entry never
// disagree about how a date looks.

/**
 * A date with the time of day on it, for a list where several entries fall on
 * the same day and the order only makes sense with the hour showing.
 */
export function moment(iso: string): string {
	const at = new Date(iso);
	return Number.isNaN(at.getTime())
		? iso
		: `${when(iso)}, ${at.toLocaleTimeString(undefined, {
				hour: "numeric",
				minute: "2-digit",
			})}`;
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
