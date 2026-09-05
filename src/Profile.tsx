// The profile screen, which has nobody on it. Aurora has no accounts, so every
// field here is a placeholder, every control is dead and every number is a
// dash: the screen is drawn now so its shape is settled before the machinery
// is. It is reachable only with SIGNED_IN flipped by hand. Nothing here is read
// or stored, and no request is missing.

import { useEffect } from "react";

import Icon from "./Icon";
import ThemeToggle from "./ThemeToggle";
import type { Theme } from "./types";

/** What a figure will be once there is an account to count it from. */
const FIGURES = [
	{ label: "Words this year" },
	{ label: "Projects" },
	{ label: "Longest streak" },
	{ label: "Member since" },
];

export default function Profile({
	onBack,
	theme,
	onTheme,
}: {
	onBack: () => void;
	theme: Theme;
	onTheme: (theme: Theme) => void;
}) {
	// The screen covers the window, so the way out has to be the key as well as
	// the button: the writing behind it cannot be clicked back to.
	useEffect(() => {
		function onKeyDown(event: KeyboardEvent) {
			if (event.key === "Escape") {
				onBack();
			}
		}

		window.addEventListener("keydown", onKeyDown);
		return () => window.removeEventListener("keydown", onKeyDown);
	}, [onBack]);

	return (
		<div className="profile">
			<div className="profile__bar">
				<button type="button" className="profile__out" onClick={onBack}>
					<Icon name="chevron-left" />
					Back to writing
				</button>
				<ThemeToggle
					theme={theme}
					onTheme={onTheme}
					className="profile__button"
				/>
			</div>

			<div className="profile__panel">
				<header className="profile__who">
					<span className="profile__avatar">
						<Icon name="user" />
					</span>
					<div className="profile__names">
						<h1 className="profile__name">Not signed in</h1>
						<p className="profile__where">
							Aurora is running on this machine only.
						</p>
					</div>
					<button type="button" className="profile__quiet" disabled>
						Change photo
					</button>
				</header>

				<div className="profile__figures">
					{FIGURES.map((figure) => (
						<div className="profile__figure" key={figure.label}>
							<p className="profile__number">—</p>
							<p className="profile__legend">{figure.label}</p>
						</div>
					))}
				</div>

				<h2 className="profile__heading">Account</h2>
				<div className="profile__card">
					<label className="profile__field">
						<span className="profile__label">Display name</span>
						<input
							type="text"
							className="profile__input"
							placeholder="Nobody yet"
							disabled
						/>
					</label>
					<label className="profile__field">
						<span className="profile__label">Email</span>
						<input
							type="email"
							className="profile__input"
							placeholder="you@example.com"
							disabled
						/>
					</label>
					<div className="profile__row">
						<span className="profile__label">Password</span>
						<button
							type="button"
							className="profile__quiet"
							disabled
						>
							Change password
						</button>
					</div>
				</div>

				<p className="profile__note">
					Accounts are not ready yet. Nothing on this screen is saved,
					and the figures above fill in once there is an account
					behind them.
				</p>

				<div className="profile__foot">
					<p className="profile__note">
						Preferences and targets are kept on this machine.
					</p>
					<button type="button" className="profile__out-of" disabled>
						Sign out
					</button>
				</div>
			</div>
		</div>
	);
}
