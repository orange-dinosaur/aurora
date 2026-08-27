// The document commands, in one place. Both the sidebar and the section
// overview offer the same actions, so each command is called from here rather
// than from whichever surface the writer happened to use.

import { invoke } from "@tauri-apps/api/core";
import type { ProjectDocument } from "./types";

/**
 * Starts a new, empty document at the end of a section. The name is the title;
 * Rust adds the extension.
 */
export function createDocument(
	root: string,
	section: string,
	name: string,
): Promise<ProjectDocument> {
	return invoke<ProjectDocument>("create_document", { root, section, name });
}
