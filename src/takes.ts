// The commands behind a document's takes.

import { invoke } from "@tauri-apps/api/core";
import type { TakeView } from "./bindings/TakeView";

/** A list that cannot be read shows no strip. The text is read apart from it. */
export function readTakes(root: string, id: string): Promise<TakeView[]> {
	return invoke<TakeView[]>("list_takes", { root, id }).catch(() => []);
}

export function takeText(
	root: string,
	id: string,
	take: string,
): Promise<string> {
	return invoke<string>("take_text", { root, id, take });
}

export function openAsTake(
	root: string,
	version: string,
	id: string,
): Promise<TakeView[]> {
	return invoke<TakeView[]>("open_as_take", { root, version, id });
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

export function renameTake(
	root: string,
	id: string,
	take: string,
	name: string,
): Promise<TakeView[]> {
	return invoke<TakeView[]>("rename_take", { root, id, take, name });
}

export function deleteTake(
	root: string,
	id: string,
	take: string,
): Promise<TakeView[]> {
	return invoke<TakeView[]>("delete_take", { root, id, take });
}

/** Only a document in the Manuscript can start a take. */
export function canTry(trail: string[]): boolean {
	return trail[0] === "Manuscript";
}
