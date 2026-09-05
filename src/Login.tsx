// The login screen, which signs nobody in. Aurora has no server behind it and
// no account to hold: this is a drawing of the form, made now so the shape of
// signing in is settled before the machinery is. Nothing typed into it is
// read, sent or stored, and the button under it cannot be pressed. Do not go
// looking for the request that is missing.

import { useEffect, useRef } from "react";

export default function Login({ onBack }: { onBack: () => void }) {
	const email = useRef<HTMLInputElement>(null);

	useEffect(() => {
		email.current?.focus();
	}, []);

	// The screen covers the window, so the way out has to be the key as well
	// as the button: the writing behind it cannot be clicked back to.
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
		<div className="login">
			<div className="login__panel">
				<h1 className="login__title">Aurora</h1>
				<p className="login__blurb">
					Sign in to keep your manuscripts, targets and preferences on
					every machine you write on.
				</p>

				<div className="login__form">
					<label className="login__field">
						<span className="login__legend">Email</span>
						<input
							ref={email}
							type="email"
							className="login__input"
							placeholder="you@example.com"
						/>
					</label>

					<label className="login__field">
						<span className="login__legend">
							Password
							<button
								type="button"
								className="login__link"
								disabled
							>
								Forgot?
							</button>
						</span>
						<input
							type="password"
							className="login__input"
							placeholder="••••••••••"
						/>
					</label>

					<button type="button" className="login__go" disabled>
						Sign in
					</button>
					<p className="login__note">
						Accounts are not ready yet. Nothing typed here is sent
						or saved.
					</p>
				</div>

				<div className="login__foot">
					<button
						type="button"
						className="login__back"
						onClick={onBack}
					>
						Keep working offline
					</button>
					<button type="button" className="login__link" disabled>
						Create an account
					</button>
				</div>
			</div>
		</div>
	);
}
