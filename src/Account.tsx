// The account, which does not exist yet. Nothing here talks to a server and
// nothing signs anybody in: this is what an account will look like when there
// is one, drawn so the shape of the app is settled before the machinery is.

import Icon from "./Icon";
import Menu, { MenuItem } from "./Menu";

/**
 * Whether the writer is signed in. Nothing writes it. Sign-in is deferred, so
 * this is always false in the app the writer runs; flipping it by hand is how
 * the signed-in half is looked at while it is being built.
 */
export const SIGNED_IN: boolean = false;

/** The chip at the right of the header, and the menu it opens. */
export function AccountChip({
	onLogin,
	onProfile,
}: {
	onLogin: () => void;
	onProfile: () => void;
}) {
	return (
		<Menu
			label={SIGNED_IN ? "Account" : "Not signed in"}
			wrapper="titlebar__menu titlebar__menu--account"
			className="account__chip"
			trigger={
				<>
					<span className="account__avatar">
						<Icon name="user" />
					</span>
					{SIGNED_IN ? "Account" : "Sign in"}
					<Icon name="chevron-down" />
				</>
			}
		>
			{(close) => (
				<>
					<div className="account__who">
						<span className="account__avatar account__avatar--big">
							<Icon name="user" />
						</span>
						<span className="account__names">
							<span className="account__name">
								{SIGNED_IN ? "Signed in" : "Not signed in"}
							</span>
							<span className="account__where">
								Working offline on this machine
							</span>
						</span>
					</div>

					<span className="menu__rule" />

					{/* The one way to the profile screen, which is why it is
					    dead until SIGNED_IN is flipped: there is no profile to
					    show a writer who has no account. */}
					<MenuItem
						disabled={!SIGNED_IN}
						onSelect={() => {
							close();
							onProfile();
						}}
					>
						Your profile
					</MenuItem>
					<MenuItem
						disabled={SIGNED_IN}
						onSelect={() => {
							close();
							onLogin();
						}}
					>
						{SIGNED_IN ? "Sign out" : "Sign in to Aurora"}
					</MenuItem>
				</>
			)}
		</Menu>
	);
}

/** The Account section of the settings dialog. */
export default function Account({ onLogin }: { onLogin: () => void }) {
	return (
		<>
			<div className="account__card">
				<span className="account__avatar account__avatar--card">
					<Icon name="user" />
				</span>
				<span className="account__names">
					<span className="account__name">
						{SIGNED_IN ? "Signed in" : "Not signed in"}
					</span>
					<span className="account__where">
						Preferences are saved on this machine only.
					</span>
				</span>
				<button
					type="button"
					className="account__in"
					disabled={SIGNED_IN}
					onClick={onLogin}
				>
					Sign in
				</button>
			</div>

			<p className="account__blurb">
				An account will sync your manuscripts, session targets,
				typography and theme between machines. Accounts are not ready
				yet, and everything else in Aurora works without one.
			</p>

			<span className="settings__rule" />

			{/* The switch is drawn where it will live and cannot be moved:
			    there is nothing behind it to turn on. */}
			<div className="settings__row">
				<div className="settings__about">
					<p className="settings__label">Sync manuscripts</p>
					<p className="settings__hint">
						Off, and not yet movable. This turns on when there is an
						account behind it.
					</p>
				</div>
				<span
					className="settings__choices"
					role="group"
					aria-label="Sync manuscripts"
				>
					<button
						type="button"
						className="settings__choice"
						aria-pressed={false}
						disabled
					>
						On
					</button>
					<button
						type="button"
						className="settings__choice"
						aria-pressed={true}
						disabled
					>
						Off
					</button>
				</span>
			</div>
		</>
	);
}
