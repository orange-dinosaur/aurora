// A button that opens a short list of things to do, and the dismissal every
// such list needs. Two surfaces use it: the actions on a row, and the + that
// makes something new.

import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type {
	CSSProperties,
	KeyboardEvent as ReactKeyboardEvent,
	MouseEvent as ReactMouseEvent,
	ReactNode,
	RefObject,
} from "react";
import Icon from "./Icon";
import type { IconName } from "./Icon";
import { inward } from "./lib/overflow";

type Props = {
	label: string;
	/** Left out only by a trigger that draws its own contents. */
	icon?: IconName;
	/** Written on the trigger beside the icon, where there is room for it. */
	text?: string;
	/**
	 * Puts the icon after the text rather than before it, for a trigger that
	 * reads as a name with a chevron after it rather than a glyph with a word
	 * beside it.
	 */
	trailing?: boolean;
	/**
	 * Everything inside the trigger, for one that is more than a glyph and a
	 * word. It replaces `icon` and `text` rather than joining them.
	 */
	trigger?: ReactNode;
	/**
	 * Joined to the wrapper's class, for a menu that has to sit in a row
	 * rather than hang off the end of one, or open to a different side.
	 */
	wrapper?: string;
	/**
	 * The trigger's class, replacing the plain one rather than joining it. A
	 * trigger that is really a card or a filled button has a shape of its own,
	 * and two shapes on one element only fight.
	 */
	className?: string;
	/** Greys the trigger, with `title` saying why. */
	disabled?: boolean;
	title?: string;
	/** A right-click on the trigger, for a menu that also opens at the pointer. */
	onContextMenu?: (event: ReactMouseEvent<HTMLElement>) => void;
	/** The items, given the way to close the menu behind them. */
	children: (close: () => void) => ReactNode;
};

export type Point = { x: number; y: number };

/** A menu asked for by right-click, and the ways to open and shut it. */
export type RightClick = {
	at: Point | null;
	open: (event: ReactMouseEvent<HTMLElement>) => void;
	close: () => void;
};

/**
 * Where a right-click asked for a menu. The menu key reports no pointer, so
 * its menu hangs under the element instead.
 */
export function pointOf(event: ReactMouseEvent<HTMLElement>): Point {
	if (event.clientX === 0 && event.clientY === 0) {
		const box = event.currentTarget.getBoundingClientRect();
		return { x: box.left, y: box.bottom };
	}
	return { x: event.clientX, y: event.clientY };
}

export function useRightClick(): RightClick {
	const [at, setAt] = useState<Point | null>(null);
	return {
		at,
		open: (event) => {
			event.preventDefault();
			setAt(pointOf(event));
		},
		close: () => setAt(null),
	};
}

/**
 * Closes something open on a click outside `within`, or on Escape. It watches
 * the document rather than the thing losing focus: this webview does not focus
 * a button when it is clicked, so a blur handler would close it on mousedown
 * and the click that followed would land on whatever it was covering.
 */
export function useDismissal(
	open: boolean,
	within: RefObject<HTMLElement | null>,
	close: () => void,
	/**
	 * The button that opens it, when that sits outside `within`. Its press
	 * must not close what its click is about to toggle.
	 */
	trigger?: RefObject<HTMLElement | null>,
) {
	// Callers pass a new function every render, so the listeners read it from
	// here rather than being set up again each time.
	const closing = useRef(close);
	useEffect(() => {
		closing.current = close;
	});

	useEffect(() => {
		if (!open) {
			return;
		}

		const me = {};
		opened.push(me);

		function away(event: MouseEvent) {
			const at = event.target;
			// A right-click menu is drawn outside whatever opened it, but a
			// press in it is no press away from that.
			if (at instanceof Element && at.closest(".menu__items--pointer")) {
				return;
			}
			if (
				!(at instanceof Node) ||
				!(
					within.current?.contains(at) ||
					trigger?.current?.contains(at)
				)
			) {
				closing.current();
			}
		}

		// One press closes one thing: the newest of what is open, and only if
		// nothing nearer the key has already used it.
		function escape(event: KeyboardEvent) {
			if (
				event.key === "Escape" &&
				!event.defaultPrevented &&
				opened[opened.length - 1] === me
			) {
				event.preventDefault();
				closing.current();
			}
		}

		document.addEventListener("mousedown", away);
		document.addEventListener("keydown", escape);
		return () => {
			opened.splice(opened.indexOf(me), 1);
			document.removeEventListener("mousedown", away);
			document.removeEventListener("keydown", escape);
		};
	}, [open, within, trigger]);
}

// What is open now, oldest first, so Escape can tell which is on top.
const opened: object[] = [];

/**
 * The keys a list of items answers to: the arrows move between the enabled
 * buttons and wrap, Home and End jump, and Tab closes the list.
 */
export function walkMenu(
	event: ReactKeyboardEvent<HTMLElement>,
	close: () => void,
) {
	if (event.key === "Tab") {
		close();
		return;
	}
	const buttons = [
		...event.currentTarget.querySelectorAll<HTMLButtonElement>(
			"button:enabled",
		),
	];
	if (buttons.length === 0) {
		return;
	}
	const at = buttons.findIndex((button) => button === document.activeElement);
	const last = buttons.length - 1;
	const next = (
		{
			ArrowDown: at >= last ? 0 : at + 1,
			ArrowUp: at <= 0 ? last : at - 1,
			Home: 0,
			End: last,
		} as Record<string, number | undefined>
	)[event.key];
	if (next !== undefined) {
		event.preventDefault();
		buttons[next].focus();
	}
}

/**
 * Whether focus was left inside `list`, or nowhere, when it closed. Only then
 * does the trigger take it back: a writer who clicked into another field has
 * gone there on purpose.
 */
export function focusWasLeft(list: HTMLElement | null): boolean {
	const now = document.activeElement;
	return (
		now === null || now === document.body || list?.contains(now) === true
	);
}

/** How long a closing menu takes to fade, which the stylesheet says too. */
export const MENU_FADE = 100;

/** How long a closing dialog takes to fade, which the stylesheet says too. */
export const DIALOG_FADE = 140;

/**
 * Whether something that has been told to close should still be drawn, so it
 * has time to fade rather than vanishing the moment it is dismissed.
 */
export function useLingering(open: boolean, ms: number): boolean {
	const [lingering, setLingering] = useState(false);

	useEffect(() => {
		if (open) {
			setLingering(true);
			return;
		}

		const timer = setTimeout(() => setLingering(false), ms);
		return () => clearTimeout(timer);
	}, [open, ms]);

	return open || lingering;
}

export default function Menu({
	label,
	icon,
	text,
	trailing,
	trigger,
	wrapper,
	className,
	disabled,
	title,
	onContextMenu,
	children,
}: Props) {
	const [open, setOpen] = useState(false);
	// The keyboard reports a click with no count, and skips the motion.
	const [instant, setInstant] = useState(false);
	const drawn = useLingering(open, MENU_FADE);
	const menu = useRef<HTMLDivElement>(null);
	const items = useRef<HTMLDivElement>(null);
	const opener = useRef<HTMLButtonElement>(null);

	useDismissal(open, menu, () => setOpen(false));

	useEffect(() => {
		if (!open) {
			return;
		}

		// Both surfaces scroll themselves, so a menu near the bottom opens
		// past the edge. Asking costs nothing when it is already in view.
		items.current?.scrollIntoView({ block: "nearest" });
		// Nothing has focus after a click here, so the keyboard needs putting
		// somewhere it can walk the menu from.
		items.current
			?.querySelector<HTMLButtonElement>("button:enabled")
			?.focus();
		return () => {
			if (focusWasLeft(items.current)) {
				opener.current?.focus();
			}
		};
	}, [open]);

	const glyph = icon === undefined ? null : <Icon name={icon} />;

	return (
		<div
			ref={menu}
			className={wrapper === undefined ? "menu" : `menu ${wrapper}`}
		>
			<button
				ref={opener}
				type="button"
				className={className ?? "menu__trigger"}
				aria-label={label}
				aria-expanded={open}
				disabled={disabled}
				title={title}
				onClick={(event) => {
					setInstant(event.detail === 0);
					setOpen((was) => !was);
				}}
				onContextMenu={onContextMenu}
			>
				{trigger ??
					(trailing === true ? (
						<>
							{text}
							{glyph}
						</>
					) : (
						<>
							{glyph}
							{text}
						</>
					))}
			</button>

			{drawn && (
				<div
					ref={items}
					className="menu__items"
					role="menu"
					data-closing={open ? undefined : ""}
					data-instant={instant ? "" : undefined}
					inert={!open}
					onKeyDown={(event) => walkMenu(event, () => setOpen(false))}
				>
					{children(() => setOpen(false))}
				</div>
			)}
		</div>
	);
}

/**
 * The same list of items, opened where the writer right-clicked rather than
 * from a button of its own. It hangs from the pointer and turns back inward
 * near the window's edges.
 */
export function PointerMenu({
	at,
	label,
	onClose,
	children,
}: {
	/** Where it was asked for, in window coordinates, or null while shut. */
	at: Point | null;
	label: string;
	onClose: () => void;
	children: (close: () => void) => ReactNode;
}) {
	const open = at !== null;
	const drawn = useLingering(open, MENU_FADE);
	const items = useRef<HTMLDivElement>(null);

	useDismissal(open, items, onClose);

	// Measured before it is painted, so it never shows past the edge first,
	// and again whenever it changes size, as Move to does once its folders
	// arrive. Closing leaves it where it was while it fades. Focus goes back
	// to whatever had it when the menu opened.
	useLayoutEffect(() => {
		const box = items.current;
		if (at === null || box === null) {
			return;
		}
		const before = document.activeElement;

		function place(box: HTMLDivElement, at: Point) {
			const spot = inward(
				at,
				{ width: box.offsetWidth, height: box.offsetHeight },
				{ width: window.innerWidth, height: window.innerHeight },
			);
			box.style.left = `${spot.x}px`;
			box.style.top = `${spot.y}px`;
		}

		place(box, at);
		const resized = new ResizeObserver(() => place(box, at));
		resized.observe(box);
		box.querySelector<HTMLButtonElement>("button:enabled")?.focus();
		return () => {
			resized.disconnect();
			if (before instanceof HTMLElement && focusWasLeft(box)) {
				before.focus();
			}
		};
	}, [at]);

	if (!drawn) {
		return null;
	}

	// On the body, so no sliding or blurred ancestor becomes what `fixed` is
	// measured against. React still bubbles its events to the row that asked,
	// so a right-click inside goes no further.
	return createPortal(
		<div
			ref={items}
			className="menu__items menu__items--pointer"
			role="menu"
			aria-label={label}
			data-closing={open ? undefined : ""}
			inert={!open}
			onKeyDown={(event) => walkMenu(event, onClose)}
			onContextMenu={(event) => {
				event.preventDefault();
				event.stopPropagation();
			}}
		>
			{children(onClose)}
		</div>,
		document.body,
	);
}

/** One line in a menu. */
export function MenuItem({
	danger,
	disabled,
	autoFocus,
	checked,
	ticks,
	depth,
	title,
	onSelect,
	children,
}: {
	danger?: boolean;
	disabled?: boolean;
	/** Says why, when it is disabled for a reason the writer can change. */
	title?: string;
	autoFocus?: boolean;
	/** Given only in a menu that picks one of several. */
	checked?: boolean;
	/** In a menu that ticks any number of its items rather than one. */
	ticks?: boolean;
	/** How far in to set it, for a menu that lists a tree. */
	depth?: number;
	onSelect: (event: ReactMouseEvent) => void;
	children: ReactNode;
}) {
	return (
		<button
			type="button"
			role={
				checked === undefined
					? "menuitem"
					: ticks === true
						? "menuitemcheckbox"
						: "menuitemradio"
			}
			aria-checked={checked}
			className={
				danger === true ? "menu__item menu__item--danger" : "menu__item"
			}
			style={
				depth === undefined
					? undefined
					: ({ "--depth": depth } as CSSProperties)
			}
			disabled={disabled}
			title={title}
			autoFocus={autoFocus}
			onClick={onSelect}
		>
			{children}
		</button>
	);
}
