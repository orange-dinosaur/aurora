import { listen } from "@tauri-apps/api/event";
import { exit } from "@tauri-apps/plugin-process";
import { flushAll } from "./flushing";

/**
 * Quitting with ⌘Q never closes the window, so the flush that rides on closing
 * it does not run. Rust holds the quit open and asks here instead: write
 * everything, then quit for real. Exiting this way carries a code, which is how
 * Rust knows to let it through rather than asking again.
 */
export function flushBeforeQuitting(): () => void {
	const listening = listen("flush-before-exit", async () => {
		await flushAll();
		await exit(0);
	});
	return () => {
		void listening.then((stop) => stop());
	};
}
