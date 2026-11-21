// The list down the left of the compare and merge screens. In a narrow window
// it floats over the page and closes like a menu.

import { type ReactNode, type RefObject, useRef } from "react";
import { useDismissal } from "./Menu";

interface Props {
	label: string;
	/** The book menu, which leads the list as it leads the binder. */
	book: ReactNode;
	/** Set while the list floats over the page in a narrow window. */
	floating: boolean;
	onDismiss: () => void;
	/** The bar's button, which a floating list treats as part of itself. */
	opener: RefObject<HTMLButtonElement | null>;
	head: ReactNode;
	foot?: ReactNode;
	children: ReactNode;
}

export default function SideList({
	label,
	book,
	floating,
	onDismiss,
	opener,
	head,
	foot,
	children,
}: Props) {
	const list = useRef<HTMLElement>(null);
	useDismissal(floating, list, onDismiss, opener);
	return (
		<nav
			ref={list}
			className={floating ? "sidelist sidelist--open" : "sidelist"}
			aria-label={label}
		>
			<div className="binder__head">{book}</div>
			<div className="sidelist__head">{head}</div>
			<div className="sidelist__list">{children}</div>
			{foot !== undefined && <div className="sidelist__foot">{foot}</div>}
		</nav>
	);
}
