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

/** `project::Role` */
export type Role =
	| "editor"
	| "copyEditor"
	| "proofreader"
	| "coverDesigner"
	| "illustrator"
	| "translator"
	| "narrator";

/** `project::Contributor` */
export type Contributor = {
	name: string;
	role: Role;
};

/**
 * `project::Book` — what the project's one book says about itself. Everything
 * is the writer's to fill in and may be empty; the identifier is generated once
 * and is not theirs to change.
 */
export type Book = {
	identifier: string;
	title: string;
	subtitle: string;
	author: string;
	contributors: Contributor[];
	series: string;
	seriesNumber: string;
	/** A BCP 47 tag such as `en` or `pt-BR`. */
	language: string;
	blurb: string;
	/** Where the cover image is, relative to the project root. */
	cover: string;
	publisher: string;
	publicationDate: string;
	isbn: string;
	keywords: string[];
	copyright: string;
	/** The formats ticked on the Export panel. */
	exportFormats: ExportFormat[];
};

/** `project::ExportFormat` */
export type ExportFormat = "markdown" | "epub";

/** `book::Extent` — how much an export of the book would take. */
export type Extent = {
	scenes: number;
	words: number;
};

/** `book::Progress` — how far an export has got. */
export type ExportProgress =
	| { stage: "reading"; done: number; total: number; scenes: number }
	| { stage: "writing"; done: number; total: number; name: string };

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
	/** null for the same reason. */
	documents: number | null;
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
	/**
	 * The document's front matter block, fences and all, for `frontmatter.ts`
	 * to read. Empty when the file has none. Rust hands the block over rather
	 * than looking inside it, so what a field is stays defined in one place.
	 */
	front: string;
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

/**
 * `tree::FolderKind` — what a folder inside the Manuscript is. The last two are
 * what surrounds the story rather than holding part of it, and a folder is one
 * of them because of its kind and not because of what it is called.
 */
export type FolderKind = "part" | "chapter" | "front-matter" | "back-matter";

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
			/** Whether an export takes it. False dims its row and everything
			 * drawn under it. */
			inBook: boolean;
	  }
	| ({ node: "document"; inBook: boolean } & ProjectDocument);

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

/**
 * `document::FolderProgress` — how a folder stands against what it is aiming
 * at. Asked for on its own, the way its fields are.
 */
export type FolderProgress = {
	/** The words in every document below it, however deep. */
	words: number;
	target: number | null;
};

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
/**
 * `history::Session` as it is sent to be recorded. The id is missing on
 * purpose: Rust mints one for every session that arrives without it, so the
 * only copy of that decision is in one place.
 */
export type SessionRecord = {
	layer: "automatic" | "deliberate";
	/** RFC 3339, which is how the file spells both of these. */
	start: string;
	end: string;
	/** Absent unless the session was a sprint. */
	limit?: { unit: "minutes" | "words"; amount: number };
	limitMet: boolean;
	written: number;
	removed: number;
	net: number;
	/** What each document gained and lost, by document id. */
	documents: Record<string, { written: number; removed: number }>;
};

/**
 * The same record read back out of the file. Rust leaves out what a session
 * has nothing to say about, and mints an id for every one it writes.
 */
export type PastSession = Omit<SessionRecord, "limitMet" | "documents"> & {
	id: string;
	limitMet?: boolean;
	documents?: Record<string, { written: number; removed: number }>;
};

export type RightSidebarTab = "synopsis" | "info" | "mentions" | "stats";

/** `store::Theme` — whether the desktop decides how Aurora is lit, or the
 * writer does. */
export type Theme = "system" | "light" | "dark";

/** `store::ManuscriptFont` — the six faces the manuscript can be set in. Five
 * are vendored; `system` is whatever the desktop calls its interface font. */
export type ManuscriptFont =
	| "newsreader"
	| "spectral"
	| "sourceSerif"
	| "plexSans"
	| "plexMono"
	| "system";

/** What a sprint counts. */
export type SprintUnit = "words" | "minutes";

export type Preferences = {
	/** Whether the bar under the document title is showing. */
	toolbar: boolean;
	focus: boolean;
	/** Whether the caret's line is held in place and the page moves under it. */
	typewriter: boolean;
	/** Whether the list of documents is showing beside the writing. */
	sidebar: boolean;
	/** Whether the panel about the open document or folder is showing. */
	rightSidebar: boolean;
	/** Which of that panel's tabs is showing. */
	rightSidebarTab: RightSidebarTab;
	/** How wide the list of documents is, in pixels. */
	sidebarWidth: number;
	/** How wide the panel about the open document is, in pixels. */
	rightSidebarWidth: number;
	/** Whether Aurora follows the desktop's light or dark setting. */
	theme: Theme;
	/** The face the manuscript is set in. The chrome is not affected. */
	manuscriptFont: ManuscriptFont;
	/** What the Sprint field starts on, or null for an empty field. */
	defaultSprint: number | null;
	/** What that sprint counts. */
	defaultSprintUnit: SprintUnit;
	/** The target a new document is given, or null for none. */
	defaultTarget: number | null;
	/** How long a silence runs before it closes an automatic session, in
	 * minutes. */
	idleMinutes: number;
	/** Whether the titlebar shows the running session. */
	sessionClock: boolean;
	/** The width of the column of text, in characters. */
	measure: number;
	/** In pixels. */
	fontSize: number;
	/** A multiple of the font size, as CSS takes it. */
	lineHeight: number;
};
