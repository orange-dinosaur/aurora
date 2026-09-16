import { Fragment, useEffect, useRef, useState } from "react";
import { nudged, SETTINGS, type Setting } from "./lib/typography";
import type { Preferences } from "./types";

type Props = {
	preferences: Preferences;
	onPreferences: (next: Preferences) => void;
};

// How the page reads, changed from where it is read. It sits beside the title
// rather than on the formatting bar, because it is not something done to the
// text — and because the bar can be hidden.
export default function Typography({ preferences, onPreferences }: Props) {
	const [open, setOpen] = useState(false);
	const panel = useRef<HTMLDivElement>(null);

	// Watched on the document rather than through the button losing focus:
	// this webview does not focus a button when it is clicked.
	useEffect(() => {
		if (!open) {
			return;
		}

		function away(event: MouseEvent) {
			const at = event.target;
			if (!(at instanceof Node) || !panel.current?.contains(at)) {
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

	function press(setting: Setting, presses: number) {
		onPreferences(nudged(preferences, setting, presses));
	}

	return (
		<div ref={panel} className="typography">
			<button
				type="button"
				className="editor__toggle"
				aria-expanded={open}
				aria-label="Typography"
				title="Typography"
				onClick={() => setOpen((was) => !was)}
			>
				Aa
			</button>

			{open && (
				<div className="typography__panel">
					{SETTINGS.map((setting) => (
						<Fragment key={setting.id}>
							<span className="typography__label">
								{setting.label}
							</span>
							<button
								type="button"
								className="typography__step"
								aria-label={`${setting.label} down`}
								disabled={
									preferences[setting.id] <= setting.min
								}
								onClick={() => press(setting, -1)}
							>
								&minus;
							</button>
							<span
								className="typography__value"
								aria-live="polite"
							>
								{setting.show(preferences[setting.id])}
							</span>
							<button
								type="button"
								className="typography__step"
								aria-label={`${setting.label} up`}
								disabled={
									preferences[setting.id] >= setting.max
								}
								onClick={() => press(setting, 1)}
							>
								+
							</button>
						</Fragment>
					))}
				</div>
			)}
		</div>
	);
}
