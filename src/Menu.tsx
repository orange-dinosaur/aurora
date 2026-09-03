// A button that opens a short list of things to do, and the dismissal every
// such list needs. Two surfaces use it: the actions on a row, and the + that
// makes something new.

import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import Icon from "./Icon";
import type { IconName } from "./Icon";

type Props = {
	label: string;
	icon: IconName;
	/** Written on the trigger beside the icon, where there is room for it. */
	text?: string;
	/** On the trigger, beside `menu__trigger`. */
	className?: string;
	/** The items, given the way to close the menu behind them. */
	children: (close: () => void) => ReactNode;
};

export default function Menu({
	label,
	icon,
	text,
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

	return (
		<div ref={menu} className="menu">
			<button
				type="button"
				className={
					className === undefined
						? "menu__trigger"
						: `menu__trigger ${className}`
				}
				aria-label={label}
				aria-expanded={open}
				onClick={() => setOpen((was) => !was)}
			>
				<Icon name={icon} />
				{text}
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
	onSelect,
	children,
}: {
	danger?: boolean;
	disabled?: boolean;
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
			disabled={disabled}
			onClick={onSelect}
		>
			{children}
		</button>
	);
}
