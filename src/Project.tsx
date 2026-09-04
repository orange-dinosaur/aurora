import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import Editor from "./Editor";
import { useFolderFields, type FieldsHandle } from "./fields";
import { list } from "./frontmatter";
import type { About } from "./Mentions";
import Search from "./Search";
import FolderView from "./FolderView";
import RightSidebar from "./RightSidebar";
import Sidebar from "./Sidebar";
import TagView from "./TagView";
import Tabs from "./Tabs";
import type { TabView } from "./Tabs";
import Titlebar from "./Titlebar";
import Trash from "./Trash";
import type {
	DocumentText,
	FolderNode,
	Preferences,
	ProjectDocument,
	TreeNode,
} from "./types";
import { retag, tagged } from "./tags";
import type { FolderRef } from "./tree";
import { documentsOf, folderOf, rows } from "./tree";
import { deleteDocument, deleteFolder } from "./documents";
import type { Seed } from "./find";
import type { OutlineHandle } from "./outline";
// `changed` under another name: this file already has one of its own.
import {
	changed as asChange,
	ended,
	recorded,
	tick,
	wrote,
	IDLE,
	type Sessions,
	type Step,
} from "./sessions";
import { failure } from "./errors";
import { pressed, SEARCH } from "./formatting";

type Props = {
	name: string;
	root: string;
	preferences: Preferences;
	onClose: () => void;
	onPreferences: (next: Preferences) => void;
};

type Content =
	| { kind: "loading" }
	| { kind: "ready"; text: string }
	| { kind: "error"; message: string };

type Save =
	| { kind: "clean" }
	| { kind: "pending" }
	| { kind: "saving" }
	// The file has gone from under the tab, so the text here is all there is
	// left of it.
	| { kind: "missing" }
	| { kind: "failed"; message: string };

type DocumentTab = {
	kind: "document";
	key: string;
	document: ProjectDocument;
	content: Content;
	save: Save;
	/** What search asked Find to open on here, or null if nothing did. */
	seed: Seed | null;
};

type FolderTab = {
	kind: "folder";
	key: string;
	/** The folder the overview is of, and what may be made inside it. */
	folder: FolderRef;
};

type TrashTab = {
	kind: "trash";
	key: string;
};

type SearchTab = {
	kind: "search";
	key: string;
};

type TagTab = {
	kind: "tag";
	key: string;
	tag: string;
};

type Tab = DocumentTab | FolderTab | TrashTab | SearchTab | TagTab;

/**
 * What is being offered after a document was renamed out from under the tags
 * pointing at it. `from` and `to` are the titles; `ids` are the documents
 * wearing the old one.
 */
type Retagging =
	| { kind: "no" }
	| { kind: "asking"; from: string; to: string; ids: string[] }
	| { kind: "writing"; from: string; to: string; ids: string[] }
	| { kind: "failed"; message: string };

/** How long the writer has to stop typing before the tab is written to disk. */
const AUTOSAVE_MS = 800;

/**
 * One tab as the strip shows it. The kind travels with it: a folder called
 * `Notes` and a document called `Notes` are not the same tab, and the strip
 * used to have only the missing `folder/` prefix to tell them apart. Only a
 * document can be unsaved.
 */
function strip(tab: Tab): TabView {
	switch (tab.kind) {
		case "document":
			return {
				key: tab.key,
				kind: "document",
				// A tab has room for the folder holding it, not the whole trail
				// down to it.
				folder: tab.document.trail.slice(-1)[0] ?? null,
				title: tab.document.title,
				dirty: tab.save.kind !== "clean",
			};
		case "folder":
			return {
				key: tab.key,
				kind: "folder",
				folder: null,
				title: tab.folder.name,
				dirty: false,
			};
		case "trash":
			return {
				key: tab.key,
				kind: "trash",
				folder: null,
				title: "Trash",
				dirty: false,
			};
		case "search":
			return {
				key: tab.key,
				kind: "search",
				folder: null,
				title: "Search",
				dirty: false,
			};
		case "tag":
			return {
				key: tab.key,
				kind: "tag",
				folder: null,
				title: tab.tag,
				dirty: false,
			};
	}
}

async function read(root: string, id: string): Promise<Content> {
	try {
		const text = await invoke<string>("read_document", { root, id });
		return { kind: "ready", text };
	} catch (error) {
		return { kind: "error", message: failure(error).message };
	}
}

export default function Project({
	name,
	root,
	preferences,
	onClose,
	onPreferences,
}: Props) {
	const [tabs, setTabs] = useState<Tab[]>([]);
	const [activeKey, setActiveKey] = useState<string | null>(null);
	// Bumped whenever this view changes what the manifest holds, so the sidebar
	// knows to read it again.
	const [listing, setListing] = useState(0);
	// Bumped every time search is asked for, so asking again while its tab is
	// already open reaches the field rather than doing nothing.
	const [asked, setAsked] = useState(0);
	// Bumped whenever a document reaches disk, which is the other way the
	// project changes under anything holding a copy of it.
	const [written, setWritten] = useState(0);
	// The one thing a rename cannot decide on its own: what to do with the tags
	// that were pointing at the old title.
	const [retagging, setRetagging] = useState<Retagging>({ kind: "no" });
	// The open document's front matter, handed up by whichever editor is
	// showing. The panel that displays it is rendered out here, beside the
	// editor rather than inside it, so it cannot read the editor for itself.
	const [fields, setFields] = useState<FieldsHandle | null>(null);
	// The headings of the same document, and for the same reason.
	const [outline, setOutline] = useState<OutlineHandle | null>(null);
	const active = tabs.find((tab) => tab.key === activeKey) ?? null;

	// A folder's fields are read on their own rather than carried on the tab:
	// the manifest is the only copy, and `listing` says when it has moved.
	const { handle: folderFields, trouble } = useFolderFields(
		root,
		active?.kind === "folder" ? active.folder.id : null,
		listing,
	);

	// A tab's key is its own, handed out when it opens and never derived from
	// what it holds: a rename changes a document's path and a restore can
	// change its id, and neither of those is the tab being replaced.
	const keys = useRef(0);
	function freshKey() {
		keys.current += 1;
		return String(keys.current);
	}

	// One timer per open document, so a tab keeps its own countdown once the
	// writer has moved on to another one.
	const timers = useRef(new Map<string, number>());

	// Both layers of the session. A ref rather than state: a keystroke must not
	// render the project, and nothing on screen reads these yet.
	const sessions = useRef<Sessions>(IDLE);

	// What the project holds, which is where a session's net comes from. The
	// sidebar says where it stands every time it reads the manifest, and the
	// editor's changes carry it forward in between, so knowing this costs no
	// reading of its own.
	const words = useRef(0);

	// Stable on purpose: the sidebar reloads its whole tree whenever the
	// handlers it was given change.
	const measured = useCallback((total: number) => {
		words.current = total;
	}, []);

	// The layers the model left behind are what runs from here, and whatever it
	// closed goes to the history file. Nothing reports a failure: history is
	// written behind the writer's back and there is nowhere to say so.
	function keep(step: Step) {
		sessions.current = step.sessions;
		return Promise.allSettled(
			step.closed.map((session) =>
				invoke("append_session", { root, session: recorded(session) }),
			),
		);
	}

	// What a change added or removed, handed to the model, which decides which
	// sessions it opens, feeds and closes.
	function record(id: string, delta: number) {
		void keep(
			wrote(
				sessions.current,
				asChange(id, Date.now(), delta),
				words.current,
			),
		);
		words.current += delta;
	}

	// Nobody else notices that a session has gone quiet, since going quiet is
	// the writer doing nothing. A minute is fine enough for a gap of half an
	// hour, and it costs nothing when there is no session to close.
	useEffect(() => {
		const beat = window.setInterval(
			() => void keep(tick(sessions.current, Date.now(), words.current)),
			60 * 1000,
		);
		return () => window.clearInterval(beat);
	}, [root]);

	// The tabs as they stand now. The handlers below are registered once and
	// would otherwise go on seeing the tabs they were born with.
	const latest = useRef(tabs);
	useEffect(() => {
		latest.current = tabs;
	});

	function documentTab(id: string): DocumentTab | undefined {
		return tabs.find(
			(tab): tab is DocumentTab =>
				tab.kind === "document" && tab.document.id === id,
		);
	}

	// Puts a tab's text on disk, putting the file itself back if it has gone.
	function put(tab: DocumentTab, text: string) {
		return tab.save.kind === "missing"
			? invoke("restore_document", {
					root,
					path: tab.document.path,
					text,
				})
			: invoke("write_document", { root, id: tab.document.id, text });
	}

	// Writes every tab that is not on disk yet, cancelling the timers that were
	// going to do it. Nothing reports a failure here: by the time this runs
	// there is no longer anywhere to report it.
	async function flush() {
		timers.current.forEach(window.clearTimeout);
		timers.current.clear();

		await Promise.allSettled(
			latest.current.flatMap((tab) =>
				tab.kind === "document" &&
				tab.save.kind !== "clean" &&
				tab.content.kind === "ready"
					? [put(tab, tab.content.text)]
					: [],
			),
		);

		// The session the writer is in the middle of is worth as much as the
		// text they were typing into it, and closing the window ends it.
		await keep(ended(sessions.current, Date.now(), words.current));
	}

	// Quitting must not lose what the debounce has not written yet. Tauri waits
	// for this handler before it closes the window, so awaiting the writes here
	// is what holds the door.
	useEffect(() => {
		const stopping = getCurrentWindow().onCloseRequested(() => flush());
		return () => {
			void stopping.then((stop) => stop());
		};
	}, [root]);

	// Closing the project unmounts this view, and loses the same edits.
	useEffect(() => {
		return () => void flush();
	}, [root]);

	function stopTimer(id: string) {
		const timer = timers.current.get(id);
		if (timer !== undefined) {
			window.clearTimeout(timer);
			timers.current.delete(id);
		}
	}

	function patch(id: string, change: (tab: DocumentTab) => DocumentTab) {
		setTabs((open) =>
			open.map((tab) =>
				tab.kind === "document" && tab.document.id === id
					? change(tab)
					: tab,
			),
		);
	}

	async function openDocument(
		document: ProjectDocument,
		seed: Seed | null = null,
	) {
		const already = documentTab(document.id);
		if (already !== undefined) {
			setActiveKey(already.key);
			// The tab was already here, so the seed is the only news: the
			// editor is mounted and Find answers it where it stands.
			if (seed !== null) {
				patch(document.id, (tab) => ({ ...tab, seed }));
			}
			return;
		}

		const opening: DocumentTab = {
			kind: "document",
			key: freshKey(),
			document,
			content: { kind: "loading" },
			save: { kind: "clean" },
			seed,
		};
		setTabs((open) => [...open, opening]);
		setActiveKey(opening.key);

		const content = await read(root, document.id);
		// Keyed by id, so a slow read can only ever fill in its own tab — and
		// quietly does nothing if that tab was closed while it was in flight.
		patch(document.id, (tab) => ({ ...tab, content }));
	}

	function openFolder(folder: FolderRef) {
		const already = tabs.find(
			(tab) => tab.kind === "folder" && tab.folder.id === folder.id,
		);
		if (already !== undefined) {
			setActiveKey(already.key);
			return;
		}

		const opening: FolderTab = {
			kind: "folder",
			key: freshKey(),
			folder,
		};
		setTabs((open) => [...open, opening]);
		setActiveKey(opening.key);
	}

	function openTrash() {
		const already = tabs.find((tab) => tab.kind === "trash");
		if (already !== undefined) {
			setActiveKey(already.key);
			return;
		}

		const opening: TrashTab = { kind: "trash", key: freshKey() };
		setTabs((open) => [...open, opening]);
		setActiveKey(opening.key);
	}

	/**
	 * Where a tag chip goes. A tag that is also the title of a document is a
	 * link to it; anything else opens the list of what wears it. The match is
	 * case-sensitive, like recognition in prose, so `elena` is a keyword even
	 * while `Elena` is a page — one rule for both rather than two answers to
	 * the same question.
	 *
	 * The title is looked up as the chip is clicked rather than held anywhere,
	 * so deleting Elena's document turns the link into a list on its own.
	 */
	async function openTag(tag: string) {
		try {
			const tree = await invoke<TreeNode[]>("document_tree", { root });
			const named = documentsOf(tree).find(
				(document) => document.title === tag,
			);
			if (named !== undefined) {
				await openDocument(named);
				return;
			}
		} catch {
			// The tree could not be read, so nothing can be said to be named
			// this. The list is the honest answer, and reports its own trouble.
		}

		const already = tabs.find(
			(tab) => tab.kind === "tag" && tab.tag === tag,
		);
		if (already !== undefined) {
			setActiveKey(already.key);
			return;
		}

		const opening: TagTab = { kind: "tag", key: freshKey(), tag };
		setTabs((open) => [...open, opening]);
		setActiveKey(opening.key);
	}

	// Reached from the window as well as from the titlebar, so it reads the
	// tabs through the ref rather than closing over them.
	function openSearch() {
		const already = latest.current.find((tab) => tab.kind === "search");
		if (already === undefined) {
			const opening: SearchTab = { kind: "search", key: freshKey() };
			setTabs((open) => [...open, opening]);
			setActiveKey(opening.key);
		} else {
			setActiveKey(already.key);
		}
		// Asking a second time is asking for the field, not for another tab.
		setAsked((times) => times + 1);
	}

	// Bound on the window, unlike every other shortcut in Aurora. The ones in
	// `Shortcuts` are registered on the editor and so are silent whenever no
	// editor has focus — which is the trash, the overviews, and search itself.
	useEffect(() => {
		function onKeyDown(event: KeyboardEvent) {
			if (pressed(event, SEARCH)) {
				event.preventDefault();
				openSearch();
			}
		}

		window.addEventListener("keydown", onKeyDown);
		return () => window.removeEventListener("keydown", onKeyDown);
	}, []);

	// A new document is opened for writing in, and every listing of it has to
	// be read again.
	function created(document: ProjectDocument) {
		void openDocument(document);
		setListing((version) => version + 1);
	}

	// Reordering, and making a folder, change the manifest and nothing else,
	// so every listing of it is told to read it again and nothing is opened.
	function changed() {
		setListing((version) => version + 1);
	}

	// A rename gives a document a new path and title but not a new id, so a
	// tab holding it is re-pointed where it stands. Its key is its own, so
	// neither the strip nor the editor is torn down for this.
	function renamed(document: ProjectDocument, was: string) {
		patch(document.id, (tab) => ({ ...tab, document }));
		setListing((version) => version + 1);

		if (was !== "" && was !== document.title) {
			void countTags(was, document.title);
		}
	}

	/**
	 * Whether anything was filed under the title this document has just left.
	 * A tag is the writer's word and not a reference Aurora maintains, so the
	 * rename lands either way and this only offers to follow it.
	 */
	async function countTags(from: string, to: string) {
		try {
			const all = await invoke<DocumentText[]>("read_all_documents", {
				root,
			});
			const ids = all.flatMap(({ id, text }) =>
				text !== null && tagged(text, from) ? [id] : [],
			);

			if (ids.length > 0) {
				setRetagging({ kind: "asking", from, to, ids });
			}
		} catch {
			// The rename itself landed. Nothing is offered if the rest of the
			// project could not be read, and nothing is said about it either:
			// a warning about a question that was never asked is noise.
		}
	}

	/**
	 * Rewrites one document's tag. A document with a tab open is written from
	 * what the tab holds rather than from the file, which may be up to 800 ms
	 * behind it, and then comes back under a fresh key: an editor seeds itself
	 * from its text once and owns it from there, so the only way to show it the
	 * new tag is to mount it again. That costs the tab its undo history, which
	 * is why nothing here touches a document that is merely open.
	 */
	async function retagOne(id: string, from: string, to: string) {
		const tab = documentTab(id);

		if (tab === undefined || tab.content.kind !== "ready") {
			const text = await invoke<string>("read_document", { root, id });
			await invoke("write_document", {
				root,
				id,
				text: retag(text, from, to),
			});
			return;
		}

		const text = retag(tab.content.text, from, to);
		stopTimer(id);
		await invoke("write_document", { root, id, text });

		const key = freshKey();
		patch(id, (open) => ({
			...open,
			key,
			content: { kind: "ready", text },
			save: { kind: "clean" },
		}));
		setActiveKey((active) => (active === tab.key ? key : active));
	}

	async function retagAll(from: string, to: string, ids: string[]) {
		setRetagging({ kind: "writing", from, to, ids });
		try {
			for (const id of ids) {
				await retagOne(id, from, to);
			}
			setRetagging({ kind: "no" });
			setWritten((times) => times + 1);
		} catch (error) {
			setRetagging({ kind: "failed", message: failure(error).message });
		}
	}

	// A move changes the path of everything under what moved, which can be a
	// whole chapter of open scenes. Rather than work out which, the tabs are
	// re-pointed from the tree: ids are what a tab holds on to, and a move
	// changes none of them.
	async function moved() {
		setListing((version) => version + 1);
		try {
			const tree = await invoke<TreeNode[]>("document_tree", { root });
			const now = new Map(documentsOf(tree).map((doc) => [doc.id, doc]));
			setTabs((open) =>
				open.map((tab) => {
					if (tab.kind !== "document") {
						return tab;
					}
					const fresh = now.get(tab.document.id);
					return fresh === undefined
						? tab
						: { ...tab, document: fresh };
				}),
			);
		} catch {
			// The listing above reads the same manifest and shows whatever went
			// wrong with it, so there is nothing to say twice.
		}
	}

	// A renamed folder keeps its id, so an overview of it is re-pointed where
	// it stands. Nothing under it moved, so the tabs holding its documents are
	// left alone: what a document is called is its own name, not its folder's.
	function renamedFolder(folder: FolderNode) {
		setTabs((open) =>
			open.map((tab) =>
				tab.kind === "folder" && tab.folder.id === folder.id
					? { ...tab, folder: { ...tab.folder, name: folder.name } }
					: tab,
			),
		);
		setListing((version) => version + 1);
	}

	// A target is the writer's intention rather than their text, so nothing is
	// saved but the manifest — and every listing of the document has to read it
	// again to show the new one on its card.
	async function retarget(id: string, target: number | null) {
		try {
			const document = await invoke<ProjectDocument>(
				"set_document_target",
				{ root, id, target },
			);
			patch(id, (tab) => ({ ...tab, document }));
			setListing((version) => version + 1);
		} catch (error) {
			patch(id, (tab) => ({
				...tab,
				save: { kind: "failed", message: failure(error).message },
			}));
		}
	}

	function edit(id: string, text: string) {
		patch(id, (tab) =>
			tab.content.kind === "ready"
				? {
						...tab,
						content: { kind: "ready", text },
						save: { kind: "pending" },
					}
				: tab,
		);

		stopTimer(id);
		timers.current.set(
			id,
			window.setTimeout(() => {
				timers.current.delete(id);
				void write(id, text);
			}, AUTOSAVE_MS),
		);
	}

	async function write(id: string, text: string) {
		patch(id, (tab) => ({ ...tab, save: { kind: "saving" } }));

		let result: Save;
		try {
			await invoke("write_document", { root, id, text });
			result = { kind: "clean" };
			setWritten((times) => times + 1);
		} catch (error) {
			const { kind, message } = failure(error);
			// The file going missing is the one failure the writer can do
			// something about, so it gets its own state rather than a message.
			result =
				kind === "documentMissing"
					? { kind: "missing" }
					: { kind: "failed", message };
		}

		// Only the write that put down what the tab still holds may report on
		// it. If the writer has typed since, a later write is already on its
		// way and this one's verdict is out of date.
		patch(id, (tab) =>
			tab.content.kind === "ready" && tab.content.text === text
				? { ...tab, save: result }
				: tab,
		);
	}

	async function restore(id: string) {
		const tab = documentTab(id);
		if (tab === undefined || tab.content.kind !== "ready") {
			return;
		}

		const text = tab.content.text;
		stopTimer(id);
		patch(id, (open) => ({ ...open, save: { kind: "saving" } }));

		let restored: ProjectDocument;
		try {
			restored = await invoke<ProjectDocument>("restore_document", {
				root,
				path: tab.document.path,
				text,
			});
		} catch (error) {
			patch(id, (open) => ({
				...open,
				save: { kind: "failed", message: failure(error).message },
			}));
			return;
		}

		// A refresh while the file was away drops the document, so it can come
		// back under a new id and the tab has to follow it.
		patch(id, (open) => ({
			...open,
			document: restored,
			save: { kind: "clean" },
		}));
		setListing((version) => version + 1);
	}

	// Takes a tab out of the strip and moves off it if it was the one being
	// looked at. Reads the tabs through `latest`, since a caller may have
	// awaited something before getting here.
	function dropTab(key: string) {
		const open = latest.current;
		const index = open.findIndex((tab) => tab.key === key);
		if (index === -1) {
			return;
		}

		const remaining = open.filter((tab) => tab.key !== key);
		setTabs(remaining);
		setActiveKey((current) =>
			current === key
				? // The one to its left, or the new first if it was leftmost.
					((remaining[index - 1] ?? remaining[0])?.key ?? null)
				: current,
		);
	}

	function closeTab(key: string) {
		const closing = tabs.find((tab) => tab.key === key);
		if (closing === undefined) {
			return;
		}

		if (closing.kind === "document") {
			stopTimer(closing.document.id);
			// Closing must not throw away what the debounce has not written
			// yet.
			if (
				closing.save.kind === "pending" &&
				closing.content.kind === "ready"
			) {
				void invoke("write_document", {
					root,
					id: closing.document.id,
					text: closing.content.text,
				});
			}
		}

		dropTab(key);
	}

	// Deleting belongs here rather than on the surface that asked for it,
	// because what the writer last typed is here. The pending write goes down
	// first so the copy in the trash is the one they were looking at, then the
	// file moves, then the tab goes. A failure is thrown back to the surface,
	// which has somewhere to show it.
	// Writes a document's tab down before its file is taken away, and hands
	// back the tab so the caller can close it once the file has gone.
	async function settle(id: string) {
		const tab = documentTab(id);
		stopTimer(id);

		if (
			tab !== undefined &&
			tab.content.kind === "ready" &&
			tab.save.kind !== "clean" &&
			// A tab whose file has gone has nothing to flush to, and putting
			// the file back only to trash it would be absurd.
			tab.save.kind !== "missing"
		) {
			await invoke("write_document", {
				root,
				id,
				text: tab.content.text,
			});
		}

		return tab;
	}

	async function remove(id: string) {
		const tab = await settle(id);
		await deleteDocument(root, id);

		if (tab !== undefined) {
			dropTab(tab.key);
		}
		setListing((version) => version + 1);
	}

	// A folder goes into the trash whole, so everything open inside it is
	// written down first and closed afterwards: its own overview, the overviews
	// of the folders under it, and every scene in any of them.
	async function removeFolder(id: string) {
		const tree = await invoke<TreeNode[]>("document_tree", { root });
		const folder = folderOf(tree, id);
		const inside = folder === null ? [] : rows(folder.children, id);

		const open = await Promise.all(
			inside.flatMap((row) =>
				row.kind === "document" ? [settle(row.document.id)] : [],
			),
		);
		await deleteFolder(root, id);

		const gone = new Set([
			id,
			...inside.flatMap((row) => (row.kind === "folder" ? [row.id] : [])),
		]);
		setTabs((tabs) =>
			tabs.filter(
				(tab) => !(tab.kind === "folder" && gone.has(tab.folder.id)),
			),
		);
		for (const tab of open) {
			if (tab !== undefined) {
				dropTab(tab.key);
			}
		}
		setListing((version) => version + 1);
	}

	// What every open document says right now, which is ahead of its file for
	// the 800 ms after a keystroke and ahead of anything search read earlier.
	const live = useMemo(
		() =>
			new Map(
				tabs.flatMap((tab) =>
					tab.kind === "document" && tab.content.kind === "ready"
						? [[tab.document.id, tab.content.text] as const]
						: [],
				),
			),
		[tabs],
	);

	function documentBody(tab: DocumentTab) {
		if (tab.content.kind === "ready") {
			return (
				<Editor
					title={tab.document.title}
					text={tab.content.text}
					active={tab.key === activeKey}
					dirty={tab.save.kind !== "clean"}
					saving={tab.save.kind === "saving"}
					missing={tab.save.kind === "missing"}
					error={tab.save.kind === "failed" ? tab.save.message : null}
					target={tab.document.target}
					seed={tab.seed}
					preferences={preferences}
					onChange={(text) => edit(tab.document.id, text)}
					onWrote={(delta) => record(tab.document.id, delta)}
					onFields={setFields}
					onOutline={setOutline}
					onRestore={() => void restore(tab.document.id)}
					onPreferences={onPreferences}
					onTarget={(target) =>
						void retarget(tab.document.id, target)
					}
				/>
			);
		}

		return (
			<article className="reader">
				<h2 className="reader__title">{tab.document.title}</h2>
				{tab.content.kind === "loading" ? (
					<p className="reader__note">Opening…</p>
				) : (
					<p className="reader__note reader__note--error">
						{tab.content.message}
					</p>
				)}
			</article>
		);
	}

	// Only a document and a folder overview have anything to say about
	// themselves. Search and the trash are lists of other things.
	const about =
		active?.kind === "document" || active?.kind === "folder"
			? active.kind
			: null;

	// What the right sidebar is about. A document's names come from the open
	// editor rather than from the file, so a name typed into Info is
	// recognised before it reaches disk.
	const panel: About | null =
		active?.kind === "document" && fields !== null
			? {
					kind: "document",
					page: {
						id: active.document.id,
						title: active.document.title,
						trail: active.document.trail,
						names: list(fields.fields, "names"),
					},
				}
			: active?.kind === "folder"
				? { kind: "folder", trail: active.folder.trail }
				: null;

	return (
		<section className="project">
			<Titlebar
				name={name}
				root={root}
				sidebar={preferences.sidebar}
				onSidebar={(open) =>
					onPreferences({ ...preferences, sidebar: open })
				}
				rightSidebar={about === null ? null : preferences.rightSidebar}
				onRightSidebar={(open) =>
					onPreferences({ ...preferences, rightSidebar: open })
				}
				onSearch={openSearch}
				onRefreshed={() => setListing((version) => version + 1)}
			/>

			<div className="project__body">
				<Sidebar
					hidden={!preferences.sidebar}
					root={root}
					reload={listing}
					onWords={measured}
					unsaved={tabs.flatMap((tab) =>
						tab.kind === "document" && tab.save.kind !== "clean"
							? [tab.document.id]
							: [],
					)}
					selectedId={
						active?.kind === "document" ? active.document.id : null
					}
					selectedFolder={
						active?.kind === "folder" ? active.folder.id : null
					}
					selectedTrash={active?.kind === "trash"}
					onSelect={(document) => void openDocument(document)}
					onOpenFolder={openFolder}
					onOpenTrash={openTrash}
					onCreated={created}
					onRenamed={renamed}
					onFolderRenamed={renamedFolder}
					onChanged={changed}
					onMoved={() => void moved()}
					onDelete={remove}
					onDeleteFolder={removeFolder}
					onClose={onClose}
				/>
				<div className="project__main">
					<Tabs
						tabs={tabs.map(strip)}
						activeKey={activeKey}
						onActivate={setActiveKey}
						onClose={closeTab}
					/>

					<div className="project__panes">
						{/* Every open document stays mounted and all but one
						    is hidden. An editor torn down on a tab switch
						    takes its undo history, its selection and its
						    scroll position with it. Hiding is by visibility
						    rather than display, which would drop the layout
						    box and with it the scroll position.

						    Search is here for the same reason and not the
						    same one: going to a hit and coming back is the
						    whole point of it being a tab, and a Search
						    unmounted on the way out would come back with an
						    empty field. */}
						{tabs.map((tab) =>
							tab.kind === "document" || tab.kind === "search" ? (
								<div
									key={tab.key}
									className={
										tab.key === activeKey
											? "project__pane"
											: "project__pane project__pane--hidden"
									}
								>
									{tab.kind === "document" ? (
										documentBody(tab)
									) : (
										<Search
											root={root}
											asked={asked}
											active={tab.key === activeKey}
											changed={listing + written}
											live={live}
											onOpen={(document, seed) =>
												void openDocument(
													document,
													seed,
												)
											}
										/>
									)}
								</div>
							) : null,
						)}

						{active === null ? (
							<div className="project__pane">
								<p className="project__empty">
									Choose a document to open.
								</p>
							</div>
						) : active.kind === "folder" ? (
							<div className="project__pane">
								{/* Keyed, so moving between two overviews
								    starts the new one empty rather than
								    showing the previous section's cards
								    until its read comes back. */}
								<FolderView
									key={active.key}
									root={root}
									folder={active.folder}
									reload={listing}
									onSelect={(document) =>
										void openDocument(document)
									}
									onOpenFolder={openFolder}
									onCreated={created}
									onRenamed={renamed}
									onFolderRenamed={renamedFolder}
									onChanged={changed}
									onMoved={() => void moved()}
									onDelete={remove}
									onDeleteFolder={removeFolder}
								/>
							</div>
						) : active.kind === "trash" ? (
							<div className="project__pane">
								<Trash
									key={active.key}
									root={root}
									reload={listing}
									onChanged={() =>
										setListing((version) => version + 1)
									}
								/>
							</div>
						) : active.kind === "tag" ? (
							<div className="project__pane">
								<TagView
									key={active.key}
									root={root}
									tag={active.tag}
									changed={listing + written}
									live={live}
									onOpen={(document) =>
										void openDocument(document)
									}
								/>
							</div>
						) : null}
					</div>

					{/* Over the panes rather than above them: the writing does
					    not move down to make room for a question about
					    something else. */}
					{retagging.kind !== "no" && (
						<p className="retag" role="status">
							{retagging.kind === "failed" ? (
								<span className="retag__said retag__said--error">
									{retagging.message}
								</span>
							) : (
								<span className="retag__said">
									{retagging.ids.length === 1
										? "One document is tagged"
										: `${retagging.ids.length} documents are tagged`}{" "}
									<span className="retag__tag">
										{retagging.from}
									</span>
									. Retag them{" "}
									<span className="retag__tag">
										{retagging.to}
									</span>
									?
								</span>
							)}

							{retagging.kind === "asking" && (
								<button
									type="button"
									className="retag__do"
									onClick={() =>
										void retagAll(
											retagging.from,
											retagging.to,
											retagging.ids,
										)
									}
								>
									Retag
								</button>
							)}

							<button
								type="button"
								className="retag__do"
								disabled={retagging.kind === "writing"}
								onClick={() => setRetagging({ kind: "no" })}
							>
								{retagging.kind === "asking"
									? "Leave"
									: "Close"}
							</button>
						</p>
					)}
				</div>

				{about !== null && preferences.rightSidebar && (
					<RightSidebar
						fields={about === "document" ? fields : folderFields}
						outline={about === "document" ? outline : null}
						trouble={trouble}
						about={panel}
						root={root}
						changed={listing + written}
						live={live}
						tab={preferences.rightSidebarTab}
						onTab={(tab) =>
							onPreferences({
								...preferences,
								rightSidebarTab: tab,
							})
						}
						onOpen={(document, seed) =>
							void openDocument(document, seed)
						}
						onOpenTag={(tag) => void openTag(tag)}
						onClose={() =>
							onPreferences({
								...preferences,
								rightSidebar: false,
							})
						}
					/>
				)}
			</div>
		</section>
	);
}
