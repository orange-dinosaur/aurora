// How wide the two panels beside the writing may be, and what a drag on the
// edge of one of them means. Kept apart from the components so the arithmetic
// can be tested without a window to drag in.

/** What a panel may be dragged to, in pixels. */
export type Bounds = { min: number; max: number };

/** The list of documents. It holds a whole project's names, so it is given
 * more room to grow than the panel opposite. */
export const SIDEBAR: Bounds = { min: 200, max: 460 };

/** The panel about what is open. Its floor is higher than the sidebar's
 * because its four tabs wrap onto a second line below it. */
export const RIGHT_SIDEBAR: Bounds = { min: 240, max: 480 };

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
