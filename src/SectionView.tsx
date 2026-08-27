import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { DocumentSummary, ProjectDocument } from "./types";
import { failure } from "./errors";

type Status =
	{ kind: "busy" } | { kind: "idle" } | { kind: "error"; message: string };

type Props = {
	root: string;
	folder: string;
	// Changes when the project view has altered the manifest.
	reload: number;
	onSelect: (document: ProjectDocument) => void;
};

function counted(words: number) {
	return words === 1 ? "1 word" : `${words} words`;
}

function when(modified: string): string {
	const at = new Date(modified);
	return Number.isNaN(at.getTime())
		? modified
		: at.toLocaleDateString(undefined, {
				day: "numeric",
				month: "short",
				year: "numeric",
			});
}

export default function SectionView({ root, folder, reload, onSelect }: Props) {
	const [documents, setDocuments] = useState<DocumentSummary[]>([]);
	const [status, setStatus] = useState<Status>({ kind: "busy" });

	const load = useCallback(async () => {
		setStatus({ kind: "busy" });
		try {
			setDocuments(
				await invoke<DocumentSummary[]>("section_overview", {
					root,
					section: folder,
				}),
			);
			setStatus({ kind: "idle" });
		} catch (error) {
			setStatus({ kind: "error", message: failure(error).message });
		}
	}, [root, folder]);

	useEffect(() => {
		void load();
	}, [load, reload]);

	return (
		<div className="overview">
			<h2 className="overview__title">{folder}</h2>

			{status.kind === "busy" && documents.length === 0 ? (
				<p className="overview__note">Reading…</p>
			) : documents.length === 0 ? (
				<p className="overview__note">Nothing here yet.</p>
			) : (
				<ul className="cards">
					{documents.map((document) => (
						<li key={document.id}>
							<button
								type="button"
								className="card"
								onClick={() => onSelect(document)}
							>
								<span className="card__title">
									{document.title}
								</span>
								<span className="card__excerpt">
									{document.excerpt}
								</span>
								<span className="card__meta">
									{document.modified === null
										? "This document’s file is no longer there"
										: `${counted(document.words)} · ${when(document.modified)}`}
								</span>
							</button>
						</li>
					))}
				</ul>
			)}

			<p className="overview__message" role="alert">
				{status.kind === "error" ? status.message : ""}
			</p>
		</div>
	);
}
