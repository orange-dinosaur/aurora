/** Where each `[data-settle]` element sat, by its key. */
export type Spots = Map<string, { box: DOMRect; node: HTMLElement }>;

type Box = { left: number; top: number };

export type Settling = {
	/** How long the things that stay take to slide. */
	slide: number;
	/** An easing token, such as `--ease-out`. */
	easing: string;
	/** Fade and grow arrivals from 0.97 over this long; none when absent. */
	enter?: number;
	/** Fade and shrink leavers to 0.97 over this long; none when absent. */
	leave?: number;
};

const QUIET = 150;

export function spots(root: Element | null): Spots {
	const found: Spots = new Map();
	root?.querySelectorAll<HTMLElement>("[data-settle]").forEach((node) => {
		found.set(node.dataset.settle ?? "", {
			box: node.getBoundingClientRect(),
			node,
		});
	});
	return found;
}

/** How far a thing has to be pushed back to stand where it was, or null if it stayed. */
export function shift(from: Box, to: Box): { x: number; y: number } | null {
	const x = Math.round(from.left - to.left);
	const y = Math.round(from.top - to.top);
	return x === 0 && y === 0 ? null : { x, y };
}

/**
 * Slides each `[data-settle]` element under `root` from where `from` saw it to
 * where it is now. Transform and opacity only, so nothing else lays out again.
 * Reduced motion fades whatever moved, arrived or left in place instead.
 */
export function settle(root: HTMLElement | null, from: Spots, how: Settling) {
	if (root === null) {
		return;
	}
	const quiet = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
	// The Web Animations API cannot read a custom property, so the token is
	// looked up here.
	const easing =
		getComputedStyle(root).getPropertyValue(how.easing).trim() || "ease";
	const now = spots(root);

	now.forEach(({ box, node }, key) => {
		const was = from.get(key);
		if (was === undefined) {
			if (how.enter !== undefined) {
				node.animate(
					quiet
						? [{ opacity: 0 }, { opacity: 1 }]
						: [
								{ opacity: 0, transform: "scale(0.97)" },
								{ opacity: 1, transform: "none" },
							],
					{ duration: quiet ? QUIET : how.enter, easing },
				);
			}
			return;
		}
		const by = shift(was.box, box);
		if (by === null) {
			return;
		}
		node.animate(
			quiet
				? [{ opacity: 0 }, { opacity: 1 }]
				: [
						{ transform: `translate(${by.x}px, ${by.y}px)` },
						{ transform: "none" },
					],
			{ duration: quiet ? QUIET : how.slide, easing },
		);
	});

	if (how.leave === undefined) {
		return;
	}
	// A leaver is already out of the page, so a copy of it stands where it was
	// for as long as it takes to go.
	const frame = root.getBoundingClientRect();
	from.forEach(({ box, node }, key) => {
		if (now.has(key)) {
			return;
		}
		const ghost = node.cloneNode(true) as HTMLElement;
		ghost.removeAttribute("data-settle");
		ghost.setAttribute("aria-hidden", "true");
		ghost.inert = true;
		Object.assign(ghost.style, {
			position: "absolute",
			left: `${box.left - frame.left - root.clientLeft}px`,
			top: `${box.top - frame.top - root.clientTop}px`,
			width: `${box.width}px`,
			height: `${box.height}px`,
			margin: "0",
			pointerEvents: "none",
		});
		root.append(ghost);
		ghost
			.animate(
				quiet
					? [{ opacity: 1 }, { opacity: 0 }]
					: [
							{ opacity: 1, transform: "none" },
							{ opacity: 0, transform: "scale(0.97)" },
						],
				{
					duration: quiet ? QUIET : how.leave,
					easing,
					fill: "forwards",
				},
			)
			.finished.finally(() => ghost.remove());
	});
}
