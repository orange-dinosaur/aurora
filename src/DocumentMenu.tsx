import { useEffect, useRef, useState } from "react";

type Props = {
	label: string;
	onRename: () => void;
};

export default function DocumentMenu({ label, onRename }: Props) {
	const [open, setOpen] = useState(false);
	const items = useRef<HTMLDivElement>(null);

	// The card grid scrolls itself, so a menu on the bottom row opens below
	// the edge of it. Asking for it to be brought into view costs nothing when
	// it is already there.
	useEffect(() => {
		if (open) {
			items.current?.scrollIntoView({ block: "nearest" });
		}
	}, [open]);

	return (
		<div
			className="menu"
			onBlur={(event) => {
				// Clicking away closes it. Escape below does the same, but a
				// writer reaching for the mouse cannot see Escape.
				const moved = event.relatedTarget;
				if (
					!(moved instanceof Node) ||
					!event.currentTarget.contains(moved)
				) {
					setOpen(false);
				}
			}}
			onKeyDown={(event) => {
				if (event.key === "Escape") {
					event.preventDefault();
					setOpen(false);
				}
			}}
		>
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
