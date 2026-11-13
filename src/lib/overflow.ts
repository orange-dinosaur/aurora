type Point = { x: number; y: number };
type Size = { width: number; height: number };

/**
 * Where a box opened at `at` sits so it stays in `room`. It hangs down and to
 * the right of the point, and flips to the other side of it on an axis where
 * it would run off the edge. Where neither side fits, it is pinned to the far
 * edge, or to the near one when the room is smaller than the box.
 */
export function inward(at: Point, box: Size, room: Size): Point {
	function along(start: number, size: number, space: number): number {
		if (start + size <= space) {
			return start;
		}
		return start - size >= 0 ? start - size : Math.max(0, space - size);
	}

	return {
		x: along(at.x, box.width, room.width),
		y: along(at.y, box.height, room.height),
	};
}

/**
 * What a row that scrolls sideways has out of view at each end. The pixel of
 * slack is for fractional widths, which can stop a hair short of the end.
 */
export function outOfView(
	left: number,
	scrollWidth: number,
	clientWidth: number,
): { before: boolean; after: boolean } {
	const end = scrollWidth - clientWidth;
	return { before: end > 1 && left > 0, after: end > 1 && left < end - 1 };
}
