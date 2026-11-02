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
