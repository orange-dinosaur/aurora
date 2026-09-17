import { useEffect, useRef, useState } from "react";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import Icon from "./Icon";
import { flushAll } from "./flushing";
import {
	HIDDEN,
	action,
	closable,
	message,
	offered,
	pretendFails,
	pretended,
	starting,
	stumbled,
	type Notice,
} from "./updates";

/**
 * A version to pretend is on offer, so the notice can be looked at in
 * development: `VITE_FAKE_UPDATE=0.9.9 pnpm tauri dev`. Nothing is downloaded
 * and nothing restarts.
 */
const FAKE: string | undefined = import.meta.env.DEV
	? import.meta.env.VITE_FAKE_UPDATE
	: undefined;

/** How long the preview spends pretending to download. */
const PRETEND_MS = 2000;

export default function UpdateNotice() {
	const [notice, setNotice] = useState<Notice>(HIDDEN);
	const found = useRef<Update | null>(null);

	// Once, on the way in. A build that cannot reach the feed, or finds no feed
	// at all, says nothing: an update is not something the writer asked for, so
	// failing to find one is not worth a word on screen.
	useEffect(() => {
		if (FAKE) {
			setNotice(offered(FAKE));
			return;
		}
		if (import.meta.env.DEV) {
			return;
		}

		let live = true;
		void check()
			.then((update) => {
				if (update !== null && live) {
					found.current = update;
					setNotice(offered(update.version));
				}
			})
			.catch((error: unknown) => console.error(error));

		return () => {
			live = false;
		};
	}, []);

	async function install() {
		setNotice(starting);
		try {
			// Everything half-typed has to be on disk before the application
			// is replaced underneath it.
			await flushAll();

			if (FAKE) {
				await new Promise((done) =>
					window.setTimeout(done, PRETEND_MS),
				);
				if (pretendFails(FAKE)) {
					throw new Error(`Pretending ${FAKE} failed to install.`);
				}
				setNotice(pretended);
				return;
			}

			await found.current?.downloadAndInstall();
			await relaunch();
		} catch (error) {
			console.error(error);
			setNotice(stumbled);
		}
	}

	if (notice.kind === "hidden") {
		return null;
	}

	const label = action(notice);

	return (
		<div className="update" role="status">
			<span className="update__message">{message(notice)}</span>
			{label !== null && (
				<button
					type="button"
					className="update__action"
					onClick={() => void install()}
				>
					{label}
				</button>
			)}
			{closable(notice) && (
				<button
					type="button"
					className="update__dismiss"
					aria-label="Dismiss"
					onClick={() => setNotice(HIDDEN)}
				>
					<Icon name="x" />
				</button>
			)}
		</div>
	);
}
