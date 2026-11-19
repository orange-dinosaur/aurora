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
 * the next tab as it always has. A side's ✕ passes its key, which is null for
 * an empty side, and ends the split the same way.
 */
export function close(sides: Sides, key: string | null): Sides {
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

/**
 * The tabs each side of a split has shown before the one it shows now, most
 * recent first. A null is the side standing empty. Kept only while the split
 * lasts and only while Aurora runs.
 */
export type Trail = { left: (string | null)[]; right: (string | null)[] };

export const noTrail: Trail = { left: [], right: [] };

/** Which tab is on which side, or null without a split. */
function positions(
	sides: Sides,
): { left: string | null; right: string | null } | null {
	const { current, other } = sides;
	if (other === null) {
		return null;
	}
	return other.left
		? { left: other.key, right: current }
		: { left: current, right: other.key };
}

/** Adds to the trail whatever a side stopped showing between two states. */
export function remember(trail: Trail, before: Sides, after: Sides): Trail {
	const was = positions(before);
	const now = positions(after);
	if (was === null || now === null) {
		return noTrail;
	}
	const push = (list: (string | null)[], key: string | null) => [
		key,
		...list.filter((earlier) => earlier !== key),
	];
	return {
		left: was.left === now.left ? trail.left : push(trail.left, was.left),
		right:
			was.right === now.right
				? trail.right
				: push(trail.right, was.right),
	};
}

/**
 * Closing a tab on a side brings back what that side showed before, if it is
 * still open and not on the other side. With nothing to go back to, the split
 * ends as `close` ends it.
 */
export function closeBack(
	sides: Sides,
	key: string,
	trail: Trail,
	open: (key: string) => boolean,
): Sides {
	const at = positions(sides);
	const side = at?.left === key ? "left" : at?.right === key ? "right" : null;
	if (at === null || side === null || sides.other === null) {
		return close(sides, key);
	}
	const across = side === "left" ? at.right : at.left;
	const back = trail[side].find((earlier) =>
		earlier === null
			? across !== null
			: earlier !== key && earlier !== across && open(earlier),
	);
	if (back === undefined) {
		return close(sides, key);
	}
	return sides.other.left === (side === "left")
		? { ...sides, other: { ...sides.other, key: back } }
		: { ...sides, current: back };
}

/** Which drop zone of an unsplit page `x` is in: its outer thirds, or none. */
export function third(x: number, width: number): "left" | "right" | null {
	if (x < width / 3) {
		return "left";
	}
	return x > (width * 2) / 3 ? "right" : null;
}

/**
 * A tab dropped onto the page. On an unsplit page `side` is the third it
 * landed in, and the split opens with the tab there and what was showing on
 * the other side. On a split it is the side it landed on: the tab takes that
 * side and becomes current. A tab already on the other side trades places.
 */
export function dropOn(
	sides: Sides,
	key: string,
	side: "left" | "right",
): Sides {
	const { current, other } = sides;
	if (other === null) {
		if (current === null) {
			return single(key);
		}
		return {
			current: key,
			other: {
				key: key === current ? null : current,
				left: side === "right",
			},
		};
	}

	const onCurrent = (side === "left") !== other.left;
	if (onCurrent) {
		return key === other.key
			? { current: key, other: { key: current, left: other.left } }
			: { current: key, other };
	}
	return {
		current: key,
		other: {
			key: key === current ? other.key : current,
			left: !other.left,
		},
	};
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
	/** Held down, so the press is the same one again. */
	repeat?: boolean;
};

/** Control with this toggles the split. */
export const TOGGLE_KEY = "\\";
/** Control with this starts the chord. */
export const CHORD_KEY = "w";

/** The chord's second key, and what it does. */
export const CHORD: Record<string, Action> = {
	h: "left",
	l: "right",
	w: "other",
	v: "split",
	q: "end",
};

// Lowercase, as `press` compares every key.
const MODIFIERS = new Set(["control", "shift", "alt", "meta"]);

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
	const once = pressOnce(waiting, { ...event, key: event.key.toLowerCase() });
	// A key held down acts once: the repeats are still swallowed, but move
	// nothing on.
	return event.repeat === true
		? { waiting, action: null, swallow: once.swallow }
		: once;
}

function pressOnce(
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
	if (control && event.key === TOGGLE_KEY) {
		return { waiting: false, action: "toggle", swallow: true };
	}
	if (control && event.key === CHORD_KEY) {
		return { waiting: true, action: null, swallow: true };
	}
	return { waiting: false, action: null, swallow: false };
}
