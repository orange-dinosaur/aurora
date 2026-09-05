import { useEffect, useRef, useState } from "react";
import Icon from "./Icon";

type Section =
	"account" | "appearance" | "typography" | "sessions" | "shortcuts";

/** The rail, in the order it is read. Each label is also the heading over the
 * pane, so the two cannot drift apart. */
const SECTIONS: { id: Section; label: string }[] = [
	{ id: "account", label: "Account" },
	{ id: "appearance", label: "Appearance" },
	{ id: "typography", label: "Typography" },
	{ id: "sessions", label: "Sessions & targets" },
	{ id: "shortcuts", label: "Shortcuts" },
];

type Props = {
	onClose: () => void;
};

/** Everything that follows the writer between projects, in one place over the
 * app. The sections are empty for now; each one arrives with the step that
 * fills it. */
export default function Settings({ onClose }: Props) {
	const [section, setSection] = useState<Section>("appearance");
	const dialog = useRef<HTMLDivElement>(null);

	// So Escape and the rail's arrows have somewhere to land: what had focus
	// is behind the dialog now, and a key pressed there would reach the app.
	useEffect(() => dialog.current?.focus(), []);

	useEffect(() => {
		function escape(event: KeyboardEvent) {
			if (event.key === "Escape") {
				onClose();
			}
		}

		document.addEventListener("keydown", escape);
		return () => document.removeEventListener("keydown", escape);
	}, [onClose]);

	const heading = SECTIONS.find((each) => each.id === section)?.label;

	return (
		<div
			className="scrim"
			// Clicking away closes it, as Escape does. On the press rather
			// than the click, so a selection dragged out of the dialog and
			// released on the scrim does not shut it.
			onMouseDown={(event) => {
				if (event.target === event.currentTarget) {
					onClose();
				}
			}}
		>
			<div
				className="settings"
				role="dialog"
				aria-modal="true"
				aria-label="Settings"
				ref={dialog}
				tabIndex={-1}
			>
				<nav className="settings__rail">
					<p className="settings__eyebrow">Settings</p>
					<ul className="settings__list">
						{SECTIONS.map((each) => (
							<li key={each.id}>
								<button
									type="button"
									className="settings__section"
									aria-current={
										each.id === section ? "page" : undefined
									}
									onClick={() => setSection(each.id)}
								>
									{each.label}
								</button>
							</li>
						))}
					</ul>
					<p className="settings__note">
						Preferences follow you into every project.
					</p>
				</nav>

				<div className="settings__pane">
					<div className="settings__head">
						<h2 className="settings__heading">{heading}</h2>
						<button
							type="button"
							className="settings__close"
							onClick={onClose}
							aria-label="Close settings"
							title="Close settings (Escape)"
						>
							<Icon name="x" />
						</button>
					</div>

					<div className="settings__body"></div>
				</div>
			</div>
		</div>
	);
}
