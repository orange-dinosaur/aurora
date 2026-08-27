import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import Editor from "./Editor";
import SectionView from "./SectionView";
import Sidebar from "./Sidebar";
import Tabs from "./Tabs";
import type { ProjectDocument } from "./types";
import { failure } from "./errors";

type Props = {
	name: string;
	root: string;
	onClose: () => void;
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
};

type SectionTab = {
	kind: "section";
	key: string;
	folder: string;
};

type Tab = DocumentTab | SectionTab;

/** How long the writer has to stop typing before the tab is written to disk. */
const AUTOSAVE_MS = 800;

async function read(root: string, id: string): Promise<Content> {
	try {
		const text = await invoke<string>("read_document", { root, id });
		return { kind: "ready", text };
	} catch (error) {
		return { kind: "error", message: failure(error).message };
	}
}

export default function Project({ name, root, onClose }: Props) {
	const [tabs, setTabs] = useState<Tab[]>([]);
	const [activeKey, setActiveKey] = useState<string | null>(null);
	// Bumped whenever this view changes what the manifest holds, so the sidebar
	// knows to read it again.
	const [listing, setListing] = useState(0);
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

	async function openDocument(document: ProjectDocument) {
		const already = documentTab(document.id);
		if (already !== undefined) {
			setActiveKey(already.key);
			return;
		}

		const opening: DocumentTab = {
			kind: "document",
			key: freshKey(),
			document,
			content: { kind: "loading" },
			save: { kind: "clean" },
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

	// A new document is opened for writing in, and every listing of it has to
	// be read again.
	function created(document: ProjectDocument) {
		void openDocument(document);
		setListing((version) => version + 1);
	}

	// A rename gives a document a new path and title but not a new id, so a
	// tab holding it is re-pointed where it stands. Its key is its own, so
	// neither the strip nor the editor is torn down for this.
	function renamed(document: ProjectDocument) {
		patch(document.id, (tab) => ({ ...tab, document }));
		setListing((version) => version + 1);
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

	function closeTab(key: string) {
		const index = tabs.findIndex((tab) => tab.key === key);
		if (index === -1) {
			return;
		}

		const closing = tabs[index];
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

		const remaining = tabs.filter((tab) => tab.key !== key);
		setTabs(remaining);
		if (activeKey === key) {
			// The one to its left, or the new first if it was leftmost.
			const neighbour: Tab | undefined =
				remaining[index - 1] ?? remaining[0];
			setActiveKey(neighbour?.key ?? null);
		}
	}

	function documentBody(tab: DocumentTab) {
		if (tab.content.kind === "ready") {
			return (
				// Keyed by the tab, so switching tabs gives the textarea a
				// fresh element rather than one carrying the last document's
				// scroll position and selection — and renaming or restoring
				// this one does not, since the tab is the same tab.
				<Editor
					key={tab.key}
					title={tab.document.title}
					text={tab.content.text}
					dirty={tab.save.kind !== "clean"}
					saving={tab.save.kind === "saving"}
					missing={tab.save.kind === "missing"}
					error={tab.save.kind === "failed" ? tab.save.message : null}
					onChange={(text) => edit(tab.document.id, text)}
					onRestore={() => void restore(tab.document.id)}
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
			<div className="project__body">
				<Sidebar
					name={name}
					root={root}
					reload={listing}
					selectedId={
						active?.kind === "document" ? active.document.id : null
					}
					selectedFolder={
						active?.kind === "section" ? active.folder : null
					}
					onSelect={(document) => void openDocument(document)}
					onOpenSection={openSection}
					onCreated={created}
					onClose={onClose}
				/>
				<div className="project__main">
					<Tabs
						tabs={tabs.map((tab) => ({
							key: tab.key,
							folder:
								tab.kind === "document"
									? tab.document.folder
									: null,
							title:
								tab.kind === "document"
									? tab.document.title
									: tab.folder,
							dirty:
								tab.kind === "document" &&
								tab.save.kind !== "clean",
						}))}
						activeKey={activeKey}
						onActivate={setActiveKey}
						onClose={closeTab}
					/>

					{active === null ? (
						<p className="project__empty">
							Choose a document to open.
						</p>
					) : active.kind === "section" ? (
						// Keyed, so moving between two overviews starts the
						// new one empty rather than showing the previous
						// section's cards until its read comes back.
						<SectionView
							key={active.key}
							root={root}
							folder={active.folder}
							reload={listing}
							onSelect={(document) => void openDocument(document)}
							onCreated={created}
							onRenamed={renamed}
						/>
					) : (
						documentBody(active)
					)}
				</div>
			</div>
		</section>
	);
}
