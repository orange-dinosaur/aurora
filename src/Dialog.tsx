// The frame every modal dialog shares: the scrim that fades, focus that lands
// inside when it opens, Tab that stays inside, and Escape or a press on the
// scrim to leave.

import { useEffect, useRef, type ReactNode } from "react";
import { trapTab } from "./lib/trap";

export default function Dialog({
	label,
	className,
	closing,
	instant,
	onLeave,
	children,
}: {
	label: string;
	className: string;
	/** Told to close and fading, so it no longer takes anything. */
	closing: boolean;
	/** Opened from the keyboard, so it does not move. */
	instant: boolean;
	onLeave: () => void;
	children: ReactNode;
}) {
	const dialog = useRef<HTMLDivElement>(null);
	const leave = useRef(onLeave);
	useEffect(() => {
		leave.current = onLeave;
	});

	// So a key has somewhere to land: what had focus is behind the dialog now.
	useEffect(() => {
		if (!closing) {
			dialog.current?.focus();
		}
	}, [closing]);

	useEffect(() => {
		if (closing) {
			return;
		}
		function escape(event: KeyboardEvent) {
			if (event.key === "Escape") {
				leave.current();
			}
		}

		document.addEventListener("keydown", escape);
		return () => document.removeEventListener("keydown", escape);
	}, [closing]);

	return (
		<div
			className="scrim"
			data-closing={closing ? "" : undefined}
			data-instant={instant ? "" : undefined}
			inert={closing}
			// On the press rather than the click, so a selection dragged out of
			// the dialog and released on the scrim does not shut it.
			onMouseDown={(event) => {
				if (event.target === event.currentTarget) {
					onLeave();
				}
			}}
		>
			<div
				ref={dialog}
				className={className}
				role="dialog"
				aria-modal="true"
				aria-label={label}
				tabIndex={-1}
				onKeyDown={(event) =>
					trapTab(event.nativeEvent, dialog.current)
				}
			>
				{children}
			</div>
		</div>
	);
}
