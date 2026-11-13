// How wide the two panels beside the writing may be, and what a drag on the
// edge of one of them means. Kept apart from the components so the arithmetic
// can be tested without a window to drag in.

/** What a panel may be dragged to, in pixels. */
export type Bounds = { min: number; max: number };

/** The list of documents. It holds a whole project's names, so it is given
 * more room to grow than the panel opposite. */
export const SIDEBAR: Bounds = { min: 200, max: 460 };

/** The panel about what is open. */
export const RIGHT_SIDEBAR: Bounds = { min: 240, max: 480 };

/** The narrowest a side of a split may be. */
export const SIDE_MIN = 320;

/** Where the divider may go, given the room the two sides share. */
export function divider(room: number): Bounds {
	return { min: SIDE_MIN, max: Math.max(SIDE_MIN, room - SIDE_MIN) };
}

/** The left side's width as the percentage the preference keeps. */
export function share(left: number, room: number): number {
	if (room <= 0) {
		return 50;
	}
	return Math.min(100, Math.max(0, Math.round((left / room) * 100)));
}

/**
 * The width a drag has reached. Both handles sit on the inner edge of their
 * panel, against the writing, so the same movement of the pointer means
 * opposite things on the two sides: dragging right widens the left panel and
 * narrows the right one.
 *
 * `from` is the width the drag started at and `moved` how far the pointer has
 * gone since. Working from where the drag began rather than from the width now
 * is what keeps a drag that runs past the end of the range and comes back
 * from following the pointer at an offset.
 */
export function dragged(
	side: "left" | "right",
	from: number,
	moved: number,
	bounds: Bounds,
): number {
	const width = side === "left" ? from + moved : from - moved;
	return Math.min(Math.max(width, bounds.min), bounds.max);
}
