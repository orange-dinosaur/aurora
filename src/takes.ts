// The commands behind a document's takes.

import { invoke } from "@tauri-apps/api/core";
import type { TakeView } from "./bindings/TakeView";

/** A list that cannot be read shows no strip. The text is read apart from it. */
export function readTakes(root: string, id: string): Promise<TakeView[]> {
	return invoke<TakeView[]>("list_takes", { root, id }).catch(() => []);
}

export function trySomething(root: string, id: string): Promise<TakeView[]> {
	return invoke<TakeView[]>("try_something", { root, id });
}

export function chooseTake(
	root: string,
	id: string,
	take: string,
): Promise<TakeView[]> {
	return invoke<TakeView[]>("choose_take", { root, id, take });
}

/** Only a document in the Manuscript can start a take. */
export function canTry(trail: string[]): boolean {
	return trail[0] === "Manuscript";
}
