// A put back button that stops to ask when it would remove takes for good.

import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { MenuItem, useDismissal } from "./Menu";

type Props = {
	className: string;
	disabled?: boolean;
	/** The takes this would remove. Left out where it can remove none. */
	removes?: () => Promise<string[]>;
	/** Null while putting back cannot run. */
	onPutBack: (() => void) | null;
	/** Asking failed, so nothing was put back. */
	onRefused: (reason: unknown) => void;
	children: ReactNode;
};

export default function PutBack({
	className,
	disabled,
	removes,
	onPutBack,
	onRefused,
	children,
}: Props) {
	const [names, setNames] = useState<string[] | null>(null);
	const wrapper = useRef<HTMLDivElement>(null);
	const items = useRef<HTMLDivElement>(null);
	const note = useRef<HTMLParagraphElement>(null);
	const asking = names !== null;

	useDismissal(asking, wrapper, () => setNames(null));

	// The note claims the keyboard rather than a button, so Enter puts nothing
	// back. Both surfaces scroll, so one near the bottom opens past the edge.
	useEffect(() => {
		if (asking) {
			items.current?.scrollIntoView({ block: "nearest" });
			note.current?.focus();
		}
	}, [asking]);

	async function click() {
		try {
			const found = (await removes?.()) ?? [];
			if (found.length === 0) {
				onPutBack?.();
			} else {
				setNames(found);
			}
		} catch (reason) {
			onRefused(reason);
		}
	}

	return (
		<div ref={wrapper} className="putback">
			<button
				type="button"
				className={className}
				disabled={disabled === true || onPutBack === null}
				onClick={() => void click()}
			>
				{children}
			</button>
			{names !== null && (
				<div ref={items} className="menu__items" role="menu">
					<p ref={note} className="takes__note" tabIndex={-1}>
						Also removes {names.join(", ")}
					</p>
					<MenuItem
						danger
						onSelect={() => {
							setNames(null);
							onPutBack?.();
						}}
					>
						Put back
					</MenuItem>
					<MenuItem onSelect={() => setNames(null)}>Keep</MenuItem>
				</div>
			)}
		</div>
	);
}
