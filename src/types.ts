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
	/** How many words the writer is aiming at, or null if they have not said. */
	target: number | null;
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

/** `document::TrashEntry` */
export type TrashEntry = {
	/** Where it is, relative to the project's trash. */
	path: string;
	folder: string;
	title: string;
	/** RFC 3339, or null when the name carries no moment Aurora recognises. */
	deleted: string | null;
};

/** `store::Preferences` — how the writer likes to write, kept in `store.json`
 * rather than in a project, so it follows them into all of them. */
export type Preferences = {
	/** Whether the bar under the document title is showing. */
	toolbar: boolean;
	focus: boolean;
	/** Whether the caret's line is held in place and the page moves under it. */
	typewriter: boolean;
	/** The width of the column of text, in characters. */
	measure: number;
	/** In pixels. */
	fontSize: number;
	/** A multiple of the font size, as CSS takes it. */
	lineHeight: number;
};
