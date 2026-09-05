// A button that opens a short list of things to do, and the dismissal every
// such list needs. Two surfaces use it: the actions on a row, and the + that
// makes something new.

import { useEffect, useRef, useState } from "react";
import type { CSSProperties, ReactNode } from "react";
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
	/** The items, given the way to close the menu behind them. */
	children: (close: () => void) => ReactNode;
};

export default function Menu({
	label,
	icon,
	text,
	trailing,
	trigger,
	wrapper,
	className,
	children,
}: Props) {
	const [open, setOpen] = useState(false);
	const menu = useRef<HTMLDivElement>(null);
	const items = useRef<HTMLDivElement>(null);

	// Dismissal is watched on the document rather than through the menu losing
	// focus: this webview does not focus a button when it is clicked, so a
	// blur handler would close the menu on mousedown and the click that
	// followed would land on whatever the menu was covering.
	useEffect(() => {
		if (!open) {
			return;
		}

		function away(event: MouseEvent) {
			const at = event.target;
			if (!(at instanceof Node) || !menu.current?.contains(at)) {
				setOpen(false);
			}
		}

		function escape(event: KeyboardEvent) {
			if (event.key === "Escape") {
				setOpen(false);
			}
		}

		document.addEventListener("mousedown", away);
		document.addEventListener("keydown", escape);
		return () => {
			document.removeEventListener("mousedown", away);
			document.removeEventListener("keydown", escape);
		};
	}, [open]);

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
				onClick={() => setOpen((was) => !was)}
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

			{open && (
				<div ref={items} className="menu__items" role="menu">
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
	depth,
	onSelect,
	children,
}: {
	danger?: boolean;
	disabled?: boolean;
	/** How far in to set it, for a menu that lists a tree. */
	depth?: number;
	onSelect: () => void;
	children: ReactNode;
}) {
	return (
		<button
			type="button"
			role="menuitem"
			className={
				danger === true ? "menu__item menu__item--danger" : "menu__item"
			}
			style={
				depth === undefined
					? undefined
					: ({ "--depth": depth } as CSSProperties)
			}
			disabled={disabled}
			onClick={onSelect}
		>
			{children}
		</button>
	);
}
