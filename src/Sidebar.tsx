import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export type ProjectDocument = {
	id: string;
	path: string;
	title: string;
};

export type SectionDocuments = {
	folder: string;
	documents: ProjectDocument[];
};

type Status =
	{ kind: "idle" } | { kind: "busy" } | { kind: "error"; message: string };

type Props = {
	root: string;
};

export default function Sidebar({ root }: Props) {
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
				setStatus({ kind: "error", message: String(error) });
			}
		},
		[root],
	);

	useEffect(() => {
		void load("list_documents");
	}, [load]);

	return (
		<nav className="sidebar" aria-label="Documents">
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

			<div className="sidebar__sections">
				{sections.map((section) => (
					<section key={section.folder} className="section">
						<h3 className="section__title">{section.folder}</h3>
						{section.documents.length > 0 ? (
							<ul className="documents">
								{section.documents.map((doc) => (
									<li key={doc.id} className="document">
										{doc.title}
									</li>
								))}
							</ul>
						) : (
							<p className="section__empty">Nothing here yet</p>
						)}
					</section>
				))}
			</div>

			<p className="sidebar__message" role="alert">
				{status.kind === "error" ? status.message : ""}
			</p>
		</nav>
	);
}
