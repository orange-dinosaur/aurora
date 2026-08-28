import { useEffect, useRef, useState } from "react";
import Icon from "./Icon";

type Props = {
	label: string;
	// Where this document sits among the others in its section, which is what
	// says whether there is anywhere left to move it.
	index: number;
	count: number;
	onMove: (index: number) => void;
	onRename: () => void;
	onDelete: () => void;
};

export default function DocumentMenu({
	label,
	index,
	count,
	onMove,
	onRename,
	onDelete,
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
				className="menu__trigger"
				aria-label={label}
				aria-expanded={open}
				onClick={() => setOpen((was) => !was)}
			>
				<Icon name="more-vertical" />
			</button>

			{open && (
				<div ref={items} className="menu__items" role="menu">
					<button
						type="button"
						role="menuitem"
						className="menu__item"
						disabled={index === 0}
						onClick={() => {
							setOpen(false);
							onMove(index - 1);
						}}
					>
						Move up
					</button>
					<button
						type="button"
						role="menuitem"
						className="menu__item"
						disabled={index >= count - 1}
						onClick={() => {
							setOpen(false);
							onMove(index + 1);
						}}
					>
						Move down
					</button>
					<button
						type="button"
						role="menuitem"
						className="menu__item"
						onClick={() => {
							setOpen(false);
							onRename();
						}}
					>
						Rename
					</button>
					<button
						type="button"
						role="menuitem"
						className="menu__item menu__item--danger"
						onClick={() => {
							setOpen(false);
							onDelete();
						}}
					>
						Delete
					</button>
				</div>
			)}
		</div>
	);
}
