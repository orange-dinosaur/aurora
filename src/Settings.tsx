import { Fragment, useEffect, useRef, useState } from "react";
import Account from "./Account";
import Icon from "./Icon";
import { UNITS } from "./Session";
import { bounded } from "./lib/settings";
import { KEYBOARD, shortcutLabel } from "./formatting";
import { face, FACES, nudged, SETTINGS, type Setting } from "./lib/typography";
import type { Preferences, Theme } from "./types";

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

/** Light before dark before system, which is the order they are offered in
 * rather than the order the store declares them: the two that decide come
 * before the one that defers. */
const THEMES: { id: Theme; label: string }[] = [
	{ id: "light", label: "Light" },
	{ id: "dark", label: "Dark" },
	{ id: "system", label: "System" },
];

/** What each typed setting is allowed to be. These are ceilings against a slip
 * on the keyboard rather than advice: a sprint of a hundred thousand words is
 * already more than anyone means, and a gap of a day is the longest silence
 * that can still be called one session. */
const SPRINT = { min: 1, max: 100000 };
const TARGET = { min: 1, max: 1000000 };
const IDLE = { min: 1, max: 1440 };

/** What the gap falls back to when the field is emptied, since a silence has
 * to be some length. The same number the store defaults to. */
const IDLE_DEFAULT = 30;

type Props = {
	preferences: Preferences;
	onPreferences: (next: Preferences) => void;
	onClose: () => void;
	/** Leaves the dialog for the login screen. */
	onLogin: () => void;
};

/** Everything that follows the writer between projects, in one place over the
 * app. The sections are empty for now; each one arrives with the step that
 * fills it. */
export default function Settings({
	preferences,
	onPreferences,
	onClose,
	onLogin,
}: Props) {
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

					<div className="settings__body">
						{section === "account" && <Account onLogin={onLogin} />}
						{section === "appearance" && (
							<Appearance
								preferences={preferences}
								onPreferences={onPreferences}
							/>
						)}
						{section === "typography" && (
							<Typography
								preferences={preferences}
								onPreferences={onPreferences}
							/>
						)}
						{section === "sessions" && (
							<Sessions
								preferences={preferences}
								onPreferences={onPreferences}
							/>
						)}
						{section === "shortcuts" && <Shortcuts />}
					</div>
				</div>
			</div>
		</div>
	);
}

function Appearance({
	preferences,
	onPreferences,
}: {
	preferences: Preferences;
	onPreferences: (next: Preferences) => void;
}) {
	return (
		<div className="settings__row">
			<div className="settings__about">
				<p className="settings__label">Theme</p>
				<p className="settings__hint">
					System follows your desktop setting and changes at dusk with
					it.
				</p>
			</div>
			<span className="settings__choices" role="group" aria-label="Theme">
				{THEMES.map((each) => (
					<button
						key={each.id}
						type="button"
						className="settings__choice"
						aria-pressed={preferences.theme === each.id}
						onClick={() =>
							onPreferences({ ...preferences, theme: each.id })
						}
					>
						{each.label}
					</button>
				))}
			</span>
		</div>
	);
}

function Typography({
	preferences,
	onPreferences,
}: {
	preferences: Preferences;
	onPreferences: (next: Preferences) => void;
}) {
	function press(setting: Setting, presses: number) {
		onPreferences(nudged(preferences, setting, presses));
	}

	return (
		<>
			<p className="settings__label">Manuscript face</p>
			<div className="settings__faces">
				{FACES.map((each) => (
					<button
						key={each.id}
						type="button"
						className="settings__face"
						data-face={each.id}
						aria-pressed={preferences.manuscriptFont === each.id}
						onClick={() =>
							onPreferences({
								...preferences,
								manuscriptFont: each.id,
							})
						}
					>
						{/* Set in the face it names, so the writer is choosing
						    by how it reads rather than by what it is called. */}
						<span
							className="settings__sample"
							style={{ fontFamily: face(each.id) }}
						>
							Rope and salt
						</span>
						<span className="settings__family">{each.family}</span>
					</button>
				))}
			</div>

			<div className="settings__rule" />

			<div className="settings__grid">
				{SETTINGS.map((setting) => (
					<Fragment key={setting.id}>
						<div className="settings__about">
							<p className="settings__label">{setting.full}</p>
							{setting.hint !== undefined && (
								<p className="settings__hint">{setting.hint}</p>
							)}
						</div>
						<span className="settings__stepper">
							<button
								type="button"
								className="settings__step"
								aria-label={`${setting.full} down`}
								disabled={
									preferences[setting.id] <= setting.min
								}
								onClick={() => press(setting, -1)}
							>
								&minus;
							</button>
							<span
								className="settings__number"
								aria-live="polite"
							>
								{setting.show(preferences[setting.id])}
							</span>
							<button
								type="button"
								className="settings__step"
								aria-label={`${setting.full} up`}
								disabled={
									preferences[setting.id] >= setting.max
								}
								onClick={() => press(setting, 1)}
							>
								+
							</button>
						</span>
					</Fragment>
				))}
			</div>
		</>
	);
}

function Sessions({
	preferences,
	onPreferences,
}: {
	preferences: Preferences;
	onPreferences: (next: Preferences) => void;
}) {
	return (
		<>
			<div className="settings__grid">
				<div className="settings__about">
					<p className="settings__label">Default sprint</p>
					<p className="settings__hint">
						What the Sprint field starts on.
					</p>
				</div>
				<span className="settings__pair">
					<Amount
						value={preferences.defaultSprint}
						label="Default sprint"
						bounds={SPRINT}
						onValue={(defaultSprint) =>
							onPreferences({ ...preferences, defaultSprint })
						}
					/>
					<span
						className="settings__choices"
						role="group"
						aria-label="What the sprint counts"
					>
						{UNITS.map((unit) => (
							<button
								key={unit}
								type="button"
								className="settings__choice"
								aria-pressed={
									preferences.defaultSprintUnit === unit
								}
								onClick={() =>
									onPreferences({
										...preferences,
										defaultSprintUnit: unit,
									})
								}
							>
								{unit}
							</button>
						))}
					</span>
				</span>

				<div className="settings__about">
					<p className="settings__label">Target for new documents</p>
					<p className="settings__hint">Leave empty for none.</p>
				</div>
				<Amount
					value={preferences.defaultTarget}
					label="Target for new documents"
					bounds={TARGET}
					onValue={(defaultTarget) =>
						onPreferences({ ...preferences, defaultTarget })
					}
				/>

				<div className="settings__about">
					<p className="settings__label">
						Count a session closed after
					</p>
					<p className="settings__hint">
						Silence long enough to have stopped writing.
					</p>
				</div>
				<span className="settings__pair">
					<Amount
						value={preferences.idleMinutes}
						label="Count a session closed after, in minutes"
						bounds={IDLE}
						onValue={(minutes) =>
							onPreferences({
								...preferences,
								idleMinutes: minutes ?? IDLE_DEFAULT,
							})
						}
					/>
					<span className="settings__suffix">minutes</span>
				</span>
			</div>

			<div className="settings__rule" />

			<div className="settings__row">
				<div className="settings__about">
					<p className="settings__label">
						Show the session clock in the title bar
					</p>
					<p className="settings__hint">
						The only place a running session is visible with both
						sidebars hidden.
					</p>
				</div>
				<button
					type="button"
					className="settings__switch"
					role="switch"
					aria-checked={preferences.sessionClock}
					aria-label="Show the session clock in the title bar"
					onClick={() =>
						onPreferences({
							...preferences,
							sessionClock: !preferences.sessionClock,
						})
					}
				>
					<span className="settings__knob" />
				</button>
			</div>
		</>
	);
}

/**
 * A number a setting is typed into. It holds no draft of its own: every
 * keystroke is the setting, and what the field shows is what was stored, so a
 * number pulled into range shows that it was the moment it happens rather than
 * when the field is left. An empty field is a real value, not a half-typed
 * one, which is what lets a setting mean "none".
 */
function Amount({
	value,
	label,
	bounds,
	onValue,
}: {
	value: number | null;
	label: string;
	bounds: { min: number; max: number };
	onValue: (value: number | null) => void;
}) {
	return (
		<input
			className="settings__field"
			value={value === null ? "" : String(value)}
			inputMode="numeric"
			aria-label={label}
			onChange={(event) =>
				onValue(bounded(event.target.value, bounds.min, bounds.max))
			}
		/>
	);
}

/**
 * The keyboard, read from what is bound rather than written out a second time.
 * Nothing here is a control: this is where a shortcut is confirmed, not where
 * it is changed.
 */
function Shortcuts() {
	return (
		<div className="settings__keys">
			{KEYBOARD.map((group) => (
				<Fragment key={group.heading}>
					<h3 className="settings__group">{group.heading}</h3>
					{group.rows.map((row) => (
						<Fragment key={row.label}>
							<span>{row.label}</span>
							<span className="settings__chord">
								{shortcutLabel(row.keys)}
							</span>
						</Fragment>
					))}
				</Fragment>
			))}
		</div>
	);
}
