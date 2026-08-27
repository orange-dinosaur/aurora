import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import NameField from "./NameField";
import type { ProjectDocument, SectionDocuments } from "./types";
import { createDocument } from "./documents";
import { failure } from "./errors";

type Status =
	{ kind: "idle" } | { kind: "busy" } | { kind: "error"; message: string };

// At most one section is being named at a time, so the folder rides along with
// the state rather than sitting beside it.
type Naming =
	| { kind: "closed" }
	| { kind: "open"; folder: string }
	| { kind: "creating"; folder: string }
	| { kind: "refused"; folder: string; message: string };

type Props = {
	name: string;
	root: string;
	// Changes when the project view has altered the manifest.
	reload: number;
	selectedId: string | null;
	selectedFolder: string | null;
	onSelect: (document: ProjectDocument) => void;
	onOpenSection: (folder: string) => void;
	onCreated: (document: ProjectDocument) => void;
	onClose: () => void;
};

export default function Sidebar({
	name,
	root,
	reload,
	selectedId,
	selectedFolder,
	onSelect,
	onOpenSection,
	onCreated,
	onClose,
}: Props) {
	const [sections, setSections] = useState<SectionDocuments[]>([]);
	const [status, setStatus] = useState<Status>({ kind: "idle" });
	const [naming, setNaming] = useState<Naming>({ kind: "closed" });

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

	async function create(folder: string, name: string) {
		setNaming({ kind: "creating", folder });
		try {
			onCreated(await createDocument(root, folder, name));
			setNaming({ kind: "closed" });
		} catch (error) {
			// A name Rust would not take leaves the field open, holding what
			// was typed, so it can be corrected rather than typed again.
			setNaming({
				kind: "refused",
				folder,
				message: failure(error).message,
			});
		}
	}

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
							<div className="section__header">
								<h3 className="section__title">
									<button
										type="button"
										className="section__open"
										aria-current={
											section.folder === selectedFolder
												? "page"
												: undefined
										}
										onClick={() =>
											onOpenSection(section.folder)
										}
									>
										{section.folder}
									</button>
								</h3>
								<button
									type="button"
									className="section__new"
									aria-label={`New document in ${section.folder}`}
									onClick={() =>
										setNaming({
											kind: "open",
											folder: section.folder,
										})
									}
								>
									+
								</button>
							</div>

							{naming.kind !== "closed" &&
								naming.folder === section.folder && (
									<div className="section__naming">
										<NameField
											label={`Name of the new document in ${section.folder}`}
											placeholder="Chapter 2"
											busy={naming.kind === "creating"}
											error={
												naming.kind === "refused"
													? naming.message
													: null
											}
											onSubmit={(name) =>
												void create(
													section.folder,
													name,
												)
											}
											onCancel={() =>
												setNaming({ kind: "closed" })
											}
										/>
									</div>
								)}
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
