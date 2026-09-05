import { useCallback, useEffect, useState } from "react";
import type { TrashEntry } from "./types";
import { listTrash, purgeTrashEntry, restoreFromTrash } from "./documents";
import { trashed } from "./cards";
import { when } from "./dates";
import { failure } from "./errors";

type Status =
	{ kind: "busy" } | { kind: "idle" } | { kind: "error"; message: string };

// Putting a document back can be undone by deleting it again; throwing one away
// cannot, so it is asked about first.
type Asking = { kind: "no" } | { kind: "yes"; path: string };

type Props = {
	root: string;
	// Changes when the project view has altered the manifest.
	reload: number;
	onChanged: () => void;
};

export default function Trash({ root, reload, onChanged }: Props) {
	const [entries, setEntries] = useState<TrashEntry[]>([]);
	const [status, setStatus] = useState<Status>({ kind: "busy" });
	const [asking, setAsking] = useState<Asking>({ kind: "no" });

	const load = useCallback(async () => {
		setStatus({ kind: "busy" });
		try {
			setEntries(await listTrash(root));
			setStatus({ kind: "idle" });
		} catch (error) {
			setStatus({ kind: "error", message: failure(error).message });
		}
	}, [root]);

	useEffect(() => {
		void load();
	}, [load, reload]);

	// Both actions leave the list as it was and let the reload that follows
	// redraw it, so there is only ever one account of what is in the trash.
	async function act(work: Promise<unknown>) {
		setStatus({ kind: "busy" });
		try {
			await work;
			setAsking({ kind: "no" });
			onChanged();
		} catch (error) {
			setStatus({ kind: "error", message: failure(error).message });
		}
	}

	if (status.kind === "busy" && entries.length === 0) {
		return (
			<div className="overview">
				<header className="overview__header">
					<h2 className="overview__title">Trash</h2>
				</header>
				<p className="overview__note">Reading…</p>
			</div>
		);
	}

	return (
		<div className="overview">
			<header className="overview__header">
				<div className="overview__heading">
					<h2 className="overview__title">Trash</h2>
					<p className="overview__count">
						Deleted documents stay here until you empty them.
						Nothing is removed from disk on its own.
					</p>
				</div>
			</header>

			{entries.length === 0 ? (
				<p className="overview__note">Nothing has been deleted.</p>
			) : (
				<ul className="trash">
					{entries.map((entry) => (
						<li key={entry.path} className="trashed">
							<span className="trashed__what">
								<span className="trashed__title">
									{entry.title}
								</span>
								<span className="trashed__meta">
									{[
										entry.folder,
										entry.inside === null
											? null
											: trashed(entry.inside),
										entry.deleted === null
											? null
											: `deleted ${when(entry.deleted)}`,
									]
										.filter((part) => part !== null)
										.join(" · ")}
								</span>
							</span>

							<span className="trashed__actions">
								{asking.kind === "yes" &&
								asking.path === entry.path ? (
									<>
										<button
											type="button"
											className="trashed__action trashed__action--danger"
											disabled={status.kind === "busy"}
											onClick={() =>
												void act(
													purgeTrashEntry(
														root,
														entry.path,
													),
												)
											}
										>
											Delete for good
										</button>
										<button
											type="button"
											className="trashed__action"
											onClick={() =>
												setAsking({ kind: "no" })
											}
										>
											Keep
										</button>
									</>
								) : (
									<>
										<button
											type="button"
											className="trashed__action"
											disabled={status.kind === "busy"}
											onClick={() =>
												void act(
													restoreFromTrash(
														root,
														entry.path,
													),
												)
											}
										>
											Put back
										</button>
										<button
											type="button"
											className="trashed__action trashed__action--danger"
											onClick={() =>
												setAsking({
													kind: "yes",
													path: entry.path,
												})
											}
										>
											Delete now
										</button>
									</>
								)}
							</span>
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
