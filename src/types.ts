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

/**
 * `project::RecentSummary` — the store's record of a project with the size of
 * its writing flattened onto it, which is how Rust serializes it.
 */
export type RecentSummary = RecentProject & {
	/** null when the project's manifest could not be read. */
	words: number | null;
};

/** `document::DocumentView` */
export type ProjectDocument = {
	id: string;
	path: string;
	/**
	 * The folders it sits in, from its section down. Search draws the whole of
	 * it and the tab strip only the last, so it arrives as names rather than
	 * as one line of text.
	 */
	trail: string[];
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

/**
 * `document::DocumentText` — a document with the whole of its text flattened
 * onto it, which is how Rust serializes it.
 */
export type DocumentText = ProjectDocument & {
	/** null when the file could not be read, which search reports. */
	text: string | null;
};

/** `tree::FolderKind` — what a folder inside the Manuscript is. */
export type FolderKind = "part" | "chapter";

/**
 * `document::NodeView` — one entry in the project's tree, tagged so the caller
 * can switch on `node`. The top level is the project's sections; every level
 * below it is one ordered list of folders and documents, which is the order a
 * reader would meet the text in.
 */
export type TreeNode =
	| {
			node: "folder";
			id: string;
			name: string;
			/** null outside the Manuscript. */
			kind: FolderKind | null;
			children: TreeNode[];
			/** The words in every document below it, however deep. */
			words: number;
	  }
	| ({ node: "document" } & ProjectDocument);

/** The folder half of a `TreeNode`, for the commands that only return one. */
export type FolderNode = Extract<TreeNode, { node: "folder" }>;

/**
 * `document::ChildSummary` — one card in a folder's overview, tagged so the
 * caller can switch on `node`. A folder card says what it is and how much it
 * holds; a document card is the summary it has always been.
 */
export type OverviewCard =
	| {
			node: "folder";
			id: string;
			name: string;
			kind: FolderKind | null;
			/** How many nodes it holds directly. */
			children: number;
			/** The words in every document below it, however deep. */
			words: number;
	  }
	| ({ node: "document" } & DocumentSummary);

/** `document::TrashEntry` */
export type TrashEntry = {
	/** Where it is, relative to the project's trash. */
	path: string;
	folder: string;
	title: string;
	/** RFC 3339, or null when the name carries no moment Aurora recognises. */
	deleted: string | null;
	/**
	 * null when the entry is a document on its own. When it is a whole folder,
	 * how many documents went into the trash inside it.
	 */
	inside: number | null;
};

/** `store::Preferences` — how the writer likes to write, kept in `store.json`
 * rather than in a project, so it follows them into all of them. */
/** The three things the right sidebar can be showing. */
export type RightSidebarTab = "synopsis" | "info" | "mentions";

export type Preferences = {
	/** Whether the bar under the document title is showing. */
	toolbar: boolean;
	focus: boolean;
	/** Whether the caret's line is held in place and the page moves under it. */
	typewriter: boolean;
	/** Whether the headings are listed in a column beside the text. */
	outline: boolean;
	/** Whether the list of documents is showing beside the writing. */
	sidebar: boolean;
	/** Whether the panel about the open document or folder is showing. */
	rightSidebar: boolean;
	/** Which of that panel's tabs is showing. */
	rightSidebarTab: RightSidebarTab;
	/** The width of the column of text, in characters. */
	measure: number;
	/** In pixels. */
	fontSize: number;
	/** A multiple of the font size, as CSS takes it. */
	lineHeight: number;
};
