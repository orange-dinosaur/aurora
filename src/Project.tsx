import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import Editor from "./Editor";
import Sidebar from "./Sidebar";
import Tabs from "./Tabs";
import type { ProjectDocument } from "./types";

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
	| { kind: "failed"; message: string };

type Tab = {
	document: ProjectDocument;
	content: Content;
	save: Save;
};

/** How long the writer has to stop typing before the tab is written to disk. */
const AUTOSAVE_MS = 800;

async function read(root: string, id: string): Promise<Content> {
	try {
		const text = await invoke<string>("read_document", { root, id });
		return { kind: "ready", text };
	} catch (error) {
		return { kind: "error", message: String(error) };
	}
}

export default function Project({ name, root, onClose }: Props) {
	const [tabs, setTabs] = useState<Tab[]>([]);
	const [activeId, setActiveId] = useState<string | null>(null);
	const active = tabs.find((tab) => tab.document.id === activeId) ?? null;

	// One timer per tab, so a tab keeps its own countdown once the writer has
	// moved on to another one.
	const timers = useRef(new Map<string, number>());
	useEffect(() => {
		const pending = timers.current;
		return () => {
			pending.forEach(window.clearTimeout);
			pending.clear();
		};
	}, []);

	function stopTimer(id: string) {
		const timer = timers.current.get(id);
		if (timer !== undefined) {
			window.clearTimeout(timer);
			timers.current.delete(id);
		}
	}

	function patch(id: string, change: (tab: Tab) => Tab) {
		setTabs((open) =>
			open.map((tab) => (tab.document.id === id ? change(tab) : tab)),
		);
	}

	async function openDocument(document: ProjectDocument) {
		setActiveId(document.id);
		if (tabs.some((tab) => tab.document.id === document.id)) {
			return;
		}

		setTabs((open) => [
			...open,
			{ document, content: { kind: "loading" }, save: { kind: "clean" } },
		]);
		const content = await read(root, document.id);
		// Keyed by id, so a slow read can only ever fill in its own tab — and
		// quietly does nothing if that tab was closed while it was in flight.
		patch(document.id, (tab) => ({ ...tab, content }));
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
			result = { kind: "failed", message: String(error) };
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

	function closeTab(id: string) {
		const index = tabs.findIndex((tab) => tab.document.id === id);
		if (index === -1) {
			return;
		}

		const closing = tabs[index];
		stopTimer(id);
		// Closing must not throw away what the debounce has not written yet.
		if (
			closing.save.kind === "pending" &&
			closing.content.kind === "ready"
		) {
			void invoke("write_document", {
				root,
				id,
				text: closing.content.text,
			});
		}

		const remaining = tabs.filter((tab) => tab.document.id !== id);
		setTabs(remaining);
		if (activeId === id) {
			// The one to its left, or the new first if it was leftmost.
			const neighbour: Tab | undefined =
				remaining[index - 1] ?? remaining[0];
			setActiveId(neighbour?.document.id ?? null);
		}
	}

	return (
		<section className="project">
			<div className="project__body">
				<Sidebar
					name={name}
					root={root}
					selectedId={activeId}
					onSelect={(document) => void openDocument(document)}
					onClose={onClose}
				/>
				<div className="project__main">
					<Tabs
						documents={tabs.map((tab) => tab.document)}
						dirty={tabs
							.filter((tab) => tab.save.kind !== "clean")
							.map((tab) => tab.document.id)}
						activeId={activeId}
						onActivate={setActiveId}
						onClose={closeTab}
					/>

					{active === null ? (
						<p className="project__empty">
							Choose a document to open.
						</p>
					) : active.content.kind === "ready" ? (
						// Keyed, so switching tabs gives the textarea a fresh
						// element rather than one carrying the last document's
						// scroll position and selection.
						<Editor
							key={active.document.id}
							title={active.document.title}
							text={active.content.text}
							dirty={active.save.kind !== "clean"}
							saving={active.save.kind === "saving"}
							error={
								active.save.kind === "failed"
									? active.save.message
									: null
							}
							onChange={(text) => edit(active.document.id, text)}
						/>
					) : (
						<article className="reader">
							<h2 className="reader__title">
								{active.document.title}
							</h2>
							{active.content.kind === "loading" ? (
								<p className="reader__note">Opening…</p>
							) : (
								<p className="reader__note reader__note--error">
									{active.content.message}
								</p>
							)}
						</article>
					)}
				</div>
			</div>
		</section>
	);
}
