import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import DocumentMenu from "./DocumentMenu";
import NameField from "./NameField";
import type { ProjectDocument, SectionDocuments } from "./types";
import { createDocument, renameDocument, reorderDocument } from "./documents";
import { failure } from "./errors";
import { useReorder } from "./reorder";

type Status =
	{ kind: "idle" } | { kind: "busy" } | { kind: "error"; message: string };

// At most one section is being named at a time, so the folder rides along with
// the state rather than sitting beside it.
type Naming =
	| { kind: "closed" }
	| { kind: "open"; folder: string }
	| { kind: "creating"; folder: string }
	| { kind: "refused"; folder: string; message: string };

// The document the writer is retitling, and what Rust made of the last name
// they tried.
type Renaming =
	| { kind: "closed" }
	| { kind: "open"; id: string }
	| { kind: "saving"; id: string }
	| { kind: "refused"; id: string; message: string };

type Props = {
	// Styled out rather than unmounted, so a half-typed section name and the
	// sections already read are still there when it comes back.
	hidden: boolean;
	root: string;
	// Changes when the project view has altered the manifest.
	reload: number;
	selectedId: string | null;
	selectedFolder: string | null;
	selectedTrash: boolean;
	onSelect: (document: ProjectDocument) => void;
	onOpenSection: (folder: string) => void;
	onOpenTrash: () => void;
	onCreated: (document: ProjectDocument) => void;
	onRenamed: (document: ProjectDocument) => void;
	onReordered: () => void;
	// The project view owns this one: it has to write down what the writer
	// last typed before the file moves, and close the tab afterwards.
	onDelete: (id: string) => Promise<void>;
	onClose: () => void;
};

export default function Sidebar({
	hidden,
	root,
	reload,
	selectedId,
	selectedFolder,
	selectedTrash,
	onSelect,
	onOpenSection,
	onOpenTrash,
	onCreated,
	onRenamed,
	onReordered,
	onDelete,
	onClose,
}: Props) {
	const [sections, setSections] = useState<SectionDocuments[]>([]);
	const [status, setStatus] = useState<Status>({ kind: "idle" });
	const [naming, setNaming] = useState<Naming>({ kind: "closed" });
	const [renaming, setRenaming] = useState<Renaming>({ kind: "closed" });
	const reorder = useReorder((id, index) => void move(id, index));

	// Reads the manifest as it stands. Looking at the folder again is the
	// titlebar's refresh, which bumps `reload` once it has done so.
	const load = useCallback(async () => {
		setStatus({ kind: "busy" });
		try {
			setSections(
				await invoke<SectionDocuments[]>("list_documents", { root }),
			);
			setStatus({ kind: "idle" });
		} catch (error) {
			setStatus({ kind: "error", message: failure(error).message });
		}
	}, [root]);

	useEffect(() => {
		void load();
	}, [load, reload]);

	async function move(id: string, index: number) {
		try {
			await reorderDocument(root, id, index);
			onReordered();
		} catch (error) {
			setStatus({ kind: "error", message: failure(error).message });
		}
	}

	async function remove(id: string) {
		try {
			await onDelete(id);
		} catch (error) {
			setStatus({ kind: "error", message: failure(error).message });
		}
	}

	async function rename(id: string, name: string) {
		setRenaming({ kind: "saving", id });
		try {
			onRenamed(await renameDocument(root, id, name));
			setRenaming({ kind: "closed" });
		} catch (error) {
			setRenaming({
				kind: "refused",
				id,
				message: failure(error).message,
			});
		}
	}

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
		<nav
			className={hidden ? "sidebar sidebar--hidden" : "sidebar"}
			aria-label="Documents"
		>
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
									<div className="sidebar__field">
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
									{section.documents.map((doc, at) =>
										renaming.kind !== "closed" &&
										renaming.id === doc.id ? (
											<li
												key={doc.id}
												className="documents__item"
											>
												<div className="sidebar__field">
													<NameField
														label={`New name for ${doc.title}`}
														placeholder="Chapter 2"
														initial={doc.title}
														busy={
															renaming.kind ===
															"saving"
														}
														error={
															renaming.kind ===
															"refused"
																? renaming.message
																: null
														}
														onSubmit={(name) =>
															void rename(
																doc.id,
																name,
															)
														}
														onCancel={() =>
															setRenaming({
																kind: "closed",
															})
														}
													/>
												</div>
											</li>
										) : (
											<li
												key={doc.id}
												className="documents__item"
												{...reorder.item(
													doc.id,
													at,
													section.folder,
												)}
											>
												<button
													type="button"
													className="document"
													aria-current={
														doc.id === selectedId
															? "page"
															: undefined
													}
													onClick={() =>
														onSelect(doc)
													}
												>
													{doc.title}
												</button>
												<DocumentMenu
													label={`Actions for ${doc.title}`}
													index={at}
													count={
														section.documents.length
													}
													onMove={(to) =>
														void move(doc.id, to)
													}
													onRename={() =>
														setRenaming({
															kind: "open",
															id: doc.id,
														})
													}
													onDelete={() =>
														void remove(doc.id)
													}
												/>
											</li>
										),
									)}
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

			<button
				type="button"
				className="sidebar__trash"
				aria-current={selectedTrash ? "page" : undefined}
				onClick={onOpenTrash}
			>
				Trash
			</button>

			<div className="sidebar__foot">
				<button
					type="button"
					className="sidebar__close"
					onClick={onClose}
				>
					Close project
				</button>
			</div>
		</nav>
	);
}
