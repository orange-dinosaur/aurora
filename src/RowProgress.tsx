// The line along the bottom of a binder row with a word target, as long as the
// share of the target its words have reached.

import type { CSSProperties } from "react";
import { reached } from "./cards";

export function progress(words: number, target: number | null) {
	const share = reached(words, target);
	if (share === null) {
		return null;
	}
	return (
		<span
			className="row__progress"
			style={{ "--reached": share } as CSSProperties}
			aria-hidden="true"
		/>
	);
}
