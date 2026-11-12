// A button that opens a short list of things to do, and the dismissal every
// such list needs. Two surfaces use it: the actions on a row, and the + that
// makes something new.

import { useEffect, useRef, useState } from "react";
import type {
	CSSProperties,
	MouseEvent as ReactMouseEvent,
	ReactNode,
	RefObject,
} from "react";
import Icon from "./Icon";
import type { IconName } from "./Icon";

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
	/** The items, given the way to close the menu behind them. */
	children: (close: () => void) => ReactNode;
};

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
	useEffect(() => {
		if (!open) {
			return;
		}

		function away(event: MouseEvent) {
			const at = event.target;
			if (
				!(at instanceof Node) ||
				!(
					within.current?.contains(at) ||
					trigger?.current?.contains(at)
				)
			) {
				close();
			}
		}

		function escape(event: KeyboardEvent) {
			if (event.key === "Escape") {
				close();
			}
		}

		document.addEventListener("mousedown", away);
		document.addEventListener("keydown", escape);
		return () => {
			document.removeEventListener("mousedown", away);
			document.removeEventListener("keydown", escape);
		};
	}, [open, within, close, trigger]);
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
	children,
}: Props) {
	const [open, setOpen] = useState(false);
	// The keyboard reports a click with no count, and skips the motion.
	const [instant, setInstant] = useState(false);
	const drawn = useLingering(open, MENU_FADE);
	const menu = useRef<HTMLDivElement>(null);
	const items = useRef<HTMLDivElement>(null);

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
	}, [open]);

	const glyph = icon === undefined ? null : <Icon name={icon} />;

	return (
		<div
			ref={menu}
			className={wrapper === undefined ? "menu" : `menu ${wrapper}`}
		>
			<button
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
				>
					{children(() => setOpen(false))}
				</div>
			)}
		</div>
	);
}

/** One line in a menu. */
export function MenuItem({
	danger,
	disabled,
	autoFocus,
	checked,
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
	/** How far in to set it, for a menu that lists a tree. */
	depth?: number;
	onSelect: (event: ReactMouseEvent) => void;
	children: ReactNode;
}) {
	return (
		<button
			type="button"
			role={checked === undefined ? "menuitem" : "menuitemradio"}
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
