import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
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

type Tab = {
	document: ProjectDocument;
	content: Content;
};

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

	async function openDocument(document: ProjectDocument) {
		setActiveId(document.id);
		if (tabs.some((tab) => tab.document.id === document.id)) {
			return;
		}

		setTabs((open) => [
			...open,
			{ document, content: { kind: "loading" } },
		]);
		const content = await read(root, document.id);
		// Keyed by id, so a slow read can only ever fill in its own tab — and
		// quietly does nothing if that tab was closed while it was in flight.
		setTabs((open) =>
			open.map((tab) =>
				tab.document.id === document.id ? { ...tab, content } : tab,
			),
		);
	}

	function closeTab(id: string) {
		const index = tabs.findIndex((tab) => tab.document.id === id);
		if (index === -1) {
			return;
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
						activeId={activeId}
						onActivate={setActiveId}
						onClose={closeTab}
					/>

					{active === null ? (
						<p className="project__empty">
							Choose a document to read.
						</p>
					) : (
						<article className="reader">
							<h2 className="reader__title">
								{active.document.title}
							</h2>
							{active.content.kind === "loading" && (
								<p className="reader__note">Opening…</p>
							)}
							{active.content.kind === "error" && (
								<p className="reader__note reader__note--error">
									{active.content.message}
								</p>
							)}
							{active.content.kind === "ready" &&
								(active.content.text === "" ? (
									<p className="reader__note">
										This document is empty.
									</p>
								) : (
									<pre className="reader__text">
										{active.content.text}
									</pre>
								))}
						</article>
					)}
				</div>
			</div>
		</section>
	);
}
