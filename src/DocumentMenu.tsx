import { useEffect, useRef, useState } from "react";

type Props = {
	label: string;
	onRename: () => void;
};

export default function DocumentMenu({ label, onRename }: Props) {
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
		items.current?.querySelector("button")?.focus();
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
				⋮
			</button>

			{open && (
				<div ref={items} className="menu__items" role="menu">
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
				</div>
			)}
		</div>
	);
}
