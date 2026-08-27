// What the Rust commands send back. TypeScript cannot check these against the
// structs they mirror, so keep the two in step by hand; the Rust name is given
// wherever it differs.

/** `project::OpenedProject` */
export type OpenProject = {
	name: string;
	root: string;
};

/** `project::LastProject` — tagged so the caller can switch on `kind`. */
export type LastProject =
	| { kind: "none" }
	| { kind: "open"; name: string; root: string }
	| { kind: "missing"; name: string; root: string };

/** `project::FormatLayout` */
export type FormatLayout = {
	format: string;
	folders: string[];
	available: boolean;
};

/** `store::RecentProject` */
export type RecentProject = {
	name: string;
	root: string;
	lastOpened: string;
};

/** `document::DocumentView` */
export type ProjectDocument = {
	id: string;
	path: string;
	folder: string;
	title: string;
};

/**
 * `document::DocumentSummary` — the sidebar's view of a document with the
 * overview's extras flattened onto it, which is how Rust serializes it.
 */
export type DocumentSummary = ProjectDocument & {
	words: number;
	excerpt: string;
	/** RFC 3339, or null when the file could not be read at all. */
	modified: string | null;
};

/** `document::SectionDocuments` */
export type SectionDocuments = {
	folder: string;
	documents: ProjectDocument[];
};
