import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import Editor from "./Editor";
import Search from "./Search";
import SectionView from "./SectionView";
import Sidebar from "./Sidebar";
import Tabs from "./Tabs";
import Titlebar from "./Titlebar";
import Trash from "./Trash";
import type { Preferences, ProjectDocument } from "./types";
import { deleteDocument } from "./documents";
import type { Seed } from "./find";
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

type SectionTab = {
	kind: "section";
	key: string;
	folder: string;
};

type TrashTab = {
	kind: "trash";
	key: string;
};

type SearchTab = {
	kind: "search";
	key: string;
};

type Tab = DocumentTab | SectionTab | TrashTab | SearchTab;

/** How long the writer has to stop typing before the tab is written to disk. */
const AUTOSAVE_MS = 800;

/** One tab as the strip shows it. Only a document can be unsaved. */
function strip(tab: Tab) {
	switch (tab.kind) {
		case "document":
			return {
				key: tab.key,
				folder: tab.document.folder,
				title: tab.document.title,
				dirty: tab.save.kind !== "clean",
			};
		case "section":
			return {
				key: tab.key,
				folder: null,
				title: tab.folder,
				dirty: false,
			};
		case "trash":
			return { key: tab.key, folder: null, title: "Trash", dirty: false };
		case "search":
			return {
				key: tab.key,
				folder: null,
				title: "Search",
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
	const active = tabs.find((tab) => tab.key === activeKey) ?? null;

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

	function openSection(folder: string) {
		const already = tabs.find(
			(tab) => tab.kind === "section" && tab.folder === folder,
		);
		if (already !== undefined) {
			setActiveKey(already.key);
			return;
		}

		const opening: SectionTab = {
			kind: "section",
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

	// Ordering lives in the manifest, so nothing here changes except what every
	// listing of it has to be told to read again.
	function reordered() {
		setListing((version) => version + 1);
	}

	// A rename gives a document a new path and title but not a new id, so a
	// tab holding it is re-pointed where it stands. Its key is its own, so
	// neither the strip nor the editor is torn down for this.
	function renamed(document: ProjectDocument) {
		patch(document.id, (tab) => ({ ...tab, document }));
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
	async function remove(id: string) {
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

		await deleteDocument(root, id);

		if (tab !== undefined) {
			dropTab(tab.key);
		}
		setListing((version) => version + 1);
	}

	function documentBody(tab: DocumentTab) {
		if (tab.content.kind === "ready") {
			return (
				<Editor
					title={tab.document.title}
					text={tab.content.text}
					dirty={tab.save.kind !== "clean"}
					saving={tab.save.kind === "saving"}
					missing={tab.save.kind === "missing"}
					error={tab.save.kind === "failed" ? tab.save.message : null}
					target={tab.document.target}
					seed={tab.seed}
					preferences={preferences}
					onChange={(text) => edit(tab.document.id, text)}
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

	return (
		<section className="project">
			<Titlebar
				name={name}
				root={root}
				sidebar={preferences.sidebar}
				onSidebar={(open) =>
					onPreferences({ ...preferences, sidebar: open })
				}
				onSearch={openSearch}
				onRefreshed={() => setListing((version) => version + 1)}
			/>

			<div className="project__body">
				<Sidebar
					hidden={!preferences.sidebar}
					root={root}
					reload={listing}
					unsaved={tabs.flatMap((tab) =>
						tab.kind === "document" && tab.save.kind !== "clean"
							? [tab.document.id]
							: [],
					)}
					selectedId={
						active?.kind === "document" ? active.document.id : null
					}
					selectedFolder={
						active?.kind === "section" ? active.folder : null
					}
					selectedTrash={active?.kind === "trash"}
					onSelect={(document) => void openDocument(document)}
					onOpenSection={openSection}
					onOpenTrash={openTrash}
					onCreated={created}
					onRenamed={renamed}
					onReordered={reordered}
					onDelete={remove}
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
						) : active.kind === "section" ? (
							<div className="project__pane">
								{/* Keyed, so moving between two overviews
								    starts the new one empty rather than
								    showing the previous section's cards
								    until its read comes back. */}
								<SectionView
									key={active.key}
									root={root}
									folder={active.folder}
									reload={listing}
									onSelect={(document) =>
										void openDocument(document)
									}
									onCreated={created}
									onRenamed={renamed}
									onReordered={reordered}
									onDelete={remove}
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
						) : null}
					</div>
				</div>
			</div>
		</section>
	);
}
