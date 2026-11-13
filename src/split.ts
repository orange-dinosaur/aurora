// Which tabs are on the page, and on which side. Every change to a split goes
// through here, so the rules can be tested without a window.

/**
 * `current` is the tab on the side the writer is in, or null when that side is
 * empty. `other` is the second side, null when there is no split; its `key` is
 * null when that side is empty. Both sides are never empty at once.
 */
export type Sides = {
	current: string | null;
	other: { key: string | null; left: boolean } | null;
};

export type Side = "left" | "right" | "other";

/** What the split keys ask for. */
export type Action = Side | "toggle" | "split" | "end";

export const single = (current: string | null): Sides => ({
	current,
	other: null,
});

/**
 * Control+\. Splitting puts the tab showing on the left and opens an empty
 * side on the right to work in. Ending keeps the current side, or the other
 * one when the current side is empty.
 */
export function toggle(sides: Sides): Sides {
	if (sides.other === null) {
		return sides.current === null
			? sides
			: { current: null, other: { key: sides.current, left: true } };
	}
	return single(sides.current ?? sides.other.key);
}

/**
 * Open beside, and where a link inside a side lands. The tab goes on the side
 * the writer is not in, which then becomes current; an empty current side is
 * filled instead.
 */
export function openBeside(sides: Sides, key: string): Sides {
	const { current, other } = sides;
	if (other === null) {
		if (current === null) {
			return single(key);
		}
		if (current === key) {
			return toggle(sides);
		}
		return { current: key, other: { key: current, left: true } };
	}
	if (key === current) {
		return sides;
	}
	if (key === other.key) {
		return focus(sides, "other");
	}
	if (current === null) {
		return { current: key, other };
	}
	return { current: key, other: { key: current, left: !other.left } };
}

/** A click on a tab in the bar or a row in the binder: the current side. */
export function activate(sides: Sides, key: string): Sides {
	if (sides.other !== null && key === sides.other.key) {
		return focus(sides, "other");
	}
	return { ...sides, current: key };
}

/** Where a link inside a side opens: across the split if there is one. */
export function follow(sides: Sides, key: string): Sides {
	return sides.other === null ? activate(sides, key) : openBeside(sides, key);
}

/** Makes a side current. Asking for the side already current does nothing. */
export function focus(sides: Sides, side: Side): Sides {
	const { current, other } = sides;
	if (other === null) {
		return sides;
	}
	const otherSide = other.left ? "left" : "right";
	if (side !== "other" && side !== otherSide) {
		return sides;
	}
	return { current: other.key, other: { key: current, left: !other.left } };
}

/** The two tabs trade places; the current one stays current. */
export function swap(sides: Sides): Sides {
	return sides.other === null
		? sides
		: { ...sides, other: { ...sides.other, left: !sides.other.left } };
}

/**
 * A tab was closed. Closing either side's tab ends the split and the other side
 * fills the page. Without a split this has nothing to say: the caller picks
 * the next tab as it always has.
 */
export function close(sides: Sides, key: string): Sides {
	const { current, other } = sides;
	if (other === null) {
		return sides;
	}
	if (key === current) {
		return single(other.key);
	}
	if (key === other.key) {
		return single(current);
	}
	return sides;
}

/** Follows tabs given new keys, as a rename does. */
export function rekey(
	sides: Sides,
	renamed: ReadonlyMap<string, string>,
): Sides {
	const to = (key: string | null) =>
		key === null ? null : (renamed.get(key) ?? key);
	return {
		current: to(sides.current),
		other:
			sides.other === null
				? null
				: { ...sides.other, key: to(sides.other.key) },
	};
}

/** Lets go of tabs that are no longer open, as closing them would have. */
export function prune(sides: Sides, open: (key: string) => boolean): Sides {
	let kept = sides;
	if (kept.current !== null && !open(kept.current)) {
		kept = kept.other === null ? single(null) : close(kept, kept.current);
	}
	if (kept.other?.key != null && !open(kept.other.key)) {
		kept = close(kept, kept.other.key);
	}
	return kept;
}

export function act(sides: Sides, action: Action): Sides {
	switch (action) {
		case "toggle":
			return toggle(sides);
		case "split":
			return sides.other === null ? toggle(sides) : sides;
		case "end":
			return sides.other === null ? sides : toggle(sides);
		default:
			return focus(sides, action);
	}
}

/**
 * What the page shows. A split without room for two sides of `minimum` width
 * folds to its current side and keeps it until there is room again.
 */
export type Layout =
	| { kind: "single"; key: string | null }
	| { kind: "split"; left: string | null; right: string | null }
	| { kind: "folded"; key: string | null; hidden: string | null };

export function layout(sides: Sides, roomy: boolean): Layout {
	const { current, other } = sides;
	if (other === null) {
		return { kind: "single", key: current };
	}
	if (!roomy) {
		return { kind: "folded", key: current, hidden: other.key };
	}
	return other.left
		? { kind: "split", left: other.key, right: current }
		: { kind: "split", left: current, right: other.key };
}

export function fits(available: number, minimum: number): boolean {
	return available >= minimum * 2;
}

/** A key press, as much of it as the split keys care about. */
export type Press = {
	key: string;
	ctrlKey: boolean;
	metaKey: boolean;
	altKey: boolean;
	shiftKey: boolean;
};

const CHORD: Record<string, Action> = {
	h: "left",
	l: "right",
	w: "other",
	v: "split",
	q: "end",
};

const MODIFIERS = new Set(["Control", "Shift", "Alt", "Meta"]);

/**
 * The split keys, Control+\ and the Control+W chord. Literal Control on every
 * platform, the Mac included, where Command+W belongs to the window. The
 * second key of the chord counts with or without Control held, as in vim.
 * `swallow` says whether the press is the split's and should go no further.
 */
export function press(
	waiting: boolean,
	event: Press,
): { waiting: boolean; action: Action | null; swallow: boolean } {
	const control =
		event.ctrlKey && !event.metaKey && !event.altKey && !event.shiftKey;
	if (waiting) {
		if (MODIFIERS.has(event.key)) {
			return { waiting: true, action: null, swallow: false };
		}
		const action =
			event.metaKey || event.altKey ? undefined : CHORD[event.key];
		return action === undefined
			? { waiting: false, action: null, swallow: false }
			: { waiting: false, action, swallow: true };
	}
	if (control && event.key === "\\") {
		return { waiting: false, action: "toggle", swallow: true };
	}
	if (control && event.key === "w") {
		return { waiting: true, action: null, swallow: true };
	}
	return { waiting: false, action: null, swallow: false };
}
