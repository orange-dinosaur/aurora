import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ProjectDocument, SectionDocuments } from "./types";
import { failure } from "./errors";

type Status =
	{ kind: "idle" } | { kind: "busy" } | { kind: "error"; message: string };

type Props = {
	name: string;
	root: string;
	// Changes when the project view has altered the manifest.
	reload: number;
	selectedId: string | null;
	onSelect: (document: ProjectDocument) => void;
	onClose: () => void;
};

export default function Sidebar({
	name,
	root,
	reload,
	selectedId,
	onSelect,
	onClose,
}: Props) {
	const [sections, setSections] = useState<SectionDocuments[]>([]);
	const [status, setStatus] = useState<Status>({ kind: "idle" });

	// `list_documents` reads the manifest; `refresh_documents` looks at the
	// folder again first, for anything changed outside Aurora.
	const load = useCallback(
		async (command: "list_documents" | "refresh_documents") => {
			setStatus({ kind: "busy" });
			try {
				setSections(
					await invoke<SectionDocuments[]>(command, { root }),
				);
				setStatus({ kind: "idle" });
			} catch (error) {
				setStatus({ kind: "error", message: failure(error).message });
			}
		},
		[root],
	);

	useEffect(() => {
		void load("list_documents");
	}, [load, reload]);

	return (
		<nav className="sidebar" aria-label="Documents">
			<h1 className="sidebar__project" title={root}>
				{name}
			</h1>

			<div className="sidebar__header">
				<h2 className="sidebar__title">Documents</h2>
				<button
					type="button"
					className="sidebar__refresh"
					disabled={status.kind === "busy"}
					onClick={() => void load("refresh_documents")}
				>
					{status.kind === "busy" ? "Refreshing…" : "Refresh"}
				</button>
			</div>

			<div className="sidebar__list">
				<div className="sidebar__sections">
					{sections.map((section) => (
						<section key={section.folder} className="section">
							<h3 className="section__title">{section.folder}</h3>
							{section.documents.length > 0 ? (
								<ul className="documents">
									{section.documents.map((doc) => (
										<li key={doc.id}>
											<button
												type="button"
												className="document"
												aria-current={
													doc.id === selectedId
														? "page"
														: undefined
												}
												onClick={() => onSelect(doc)}
											>
												{doc.title}
											</button>
										</li>
									))}
								</ul>
							) : (
								<p className="section__empty">
									Nothing here yet
								</p>
							)}
						</section>
					))}
				</div>

				<p className="sidebar__message" role="alert">
					{status.kind === "error" ? status.message : ""}
				</p>
			</div>

			<button type="button" className="sidebar__close" onClick={onClose}>
				Close project
			</button>
		</nav>
	);
}
