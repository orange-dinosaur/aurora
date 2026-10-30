import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import Icon from "./Icon";
import { MenuItem } from "./Menu";
import NameField from "./NameField";
import { unmergedQuestion } from "./branches";
import { failure } from "./errors";
import type { Branch, Checkpoint } from "./types";

type Doing =
	| { kind: "list" }
	| { kind: "switching"; slug: string; merge: boolean }
	| { kind: "picking" }
	| { kind: "naming" }
	| { kind: "renaming"; slug: string }
	| { kind: "deleting"; version: Branch };

// Its own component so a question or a name field goes with the menu: the menu
// unmounts what it holds when it closes, however it was closed.
export default function VersionMenu({
	root,
	versions,
	current,
	merging,
	close,
	onChoose,
	onCheckpoint,
	onCompare,
	onChanged,
	onFailure,
}: {
	root: string;
	versions: Branch[];
	current: Branch;
	/** Whether a merge is waiting to be settled, which rules out another. */
	merging: boolean;
	close: () => void;
	/** Switches to the version, or merges it in. True when it has to ask first. */
	onChoose: (slug: string, merge: boolean) => Promise<boolean>;
	onCheckpoint: (slug: string, message: string, merge: boolean) => void;
	onCompare: (checkpoint: string, slug: string) => void;
	/** Lists the versions again after one was made, renamed or deleted. */
	onChanged: () => Promise<void>;
	onFailure: (message: string) => void;
}) {
	const [doing, setDoing] = useState<Doing>({ kind: "list" });
	const [words, setWords] = useState("");
	const [busy, setBusy] = useState(false);
	const [error, setError] = useState<string | null>(null);
	// Whether there is a checkpoint to start a new version from, or null
	// until that is known.
	const [kept, setKept] = useState<boolean | null>(null);

	useEffect(() => {
		let open = true;
		invoke<Checkpoint[]>("list_checkpoints", { root })
			.then((rows) => {
				if (open) {
					setKept(rows.length > 0);
				}
			})
			.catch(() => {
				// Trying will bring back whatever Rust has to say.
				if (open) {
					setKept(true);
				}
			});
		return () => {
			open = false;
		};
	}, [root]);

	function back() {
		setDoing({ kind: "list" });
		setError(null);
	}

	// Runs a name past Rust, keeping the field open with its error on a refusal.
	async function named(task: () => Promise<unknown>, then: () => void) {
		setBusy(true);
		try {
			await task();
			then();
			setError(null);
			await onChanged();
		} catch (failed) {
			setError(failure(failed).message);
		} finally {
			setBusy(false);
		}
	}

	// Opens compare at the version's latest checkpoint, against now.
	async function compare(version: Branch) {
		try {
			const [latest] = await invoke<Checkpoint[]>("list_checkpoints", {
				root,
				version: version.slug,
			});
			if (latest === undefined) {
				onFailure(`${version.name} has no checkpoints yet.`);
				return;
			}
			close();
			onCompare(latest.id, version.slug);
		} catch (failed) {
			onFailure(failure(failed).message);
		}
	}

	async function remove(slug: string) {
		try {
			await invoke("delete_branch", { root, slug });
		} catch (failed) {
			onFailure(failure(failed).message);
		}
		setDoing({ kind: "list" });
		await onChanged();
	}

	if (doing.kind === "deleting") {
		return (
			<div className="version__ask">
				<p className="version__ask-text">
					{unmergedQuestion(
						doing.version.name,
						doing.version.unmerged,
					)}
				</p>
				<span className="version__ask-buttons">
					{/* Focused rather than Delete, so Enter cannot lose work. */}
					<button
						type="button"
						className="sprint__stop"
						autoFocus
						onClick={back}
					>
						Cancel
					</button>
					<button
						type="button"
						className="sprint__stop version__danger"
						onClick={() => void remove(doing.version.slug)}
					>
						Delete
					</button>
				</span>
			</div>
		);
	}

	if (doing.kind === "switching") {
		return (
			<form
				className="version__ask"
				onSubmit={(event) => {
					event.preventDefault();
					if (words.trim() !== "") {
						close();
						onCheckpoint(doing.slug, words, doing.merge);
					}
				}}
			>
				<p className="version__ask-text">
					{current.name} has changes that aren't in a checkpoint.
				</p>
				<input
					className="namefield__input"
					autoFocus
					value={words}
					aria-label="Checkpoint message"
					placeholder="What changed"
					onChange={(event) => setWords(event.target.value)}
				/>
				<span className="version__ask-buttons">
					<button
						type="button"
						className="sprint__stop"
						onClick={close}
					>
						Cancel
					</button>
					<button
						type="submit"
						className="sprint__go"
						disabled={words.trim() === ""}
					>
						{doing.merge
							? "Add checkpoint and merge"
							: "Add checkpoint and switch"}
					</button>
				</span>
			</form>
		);
	}

	function pick(slug: string, merge: boolean) {
		void onChoose(slug, merge).then((ask) =>
			ask ? setDoing({ kind: "switching", slug, merge }) : close(),
		);
	}

	const others = versions.filter((version) => !version.current);

	if (doing.kind === "picking") {
		return (
			<>
				<p className="version__note">
					Which version goes into {current.name}?
				</p>
				{others.map((version) => (
					<MenuItem
						key={version.slug}
						onSelect={() => pick(version.slug, true)}
					>
						<span className="menu__tick" />
						{version.name}
					</MenuItem>
				))}
			</>
		);
	}

	return (
		<>
			{versions.map((version) =>
				doing.kind === "renaming" && doing.slug === version.slug ? (
					<div key={version.slug} className="version__field">
						<NameField
							label={`New name for ${version.name}`}
							placeholder="Name this version"
							initial={version.name}
							busy={busy}
							working="Renaming…"
							error={error}
							onSubmit={(name) =>
								void named(
									() =>
										invoke("rename_branch", {
											root,
											slug: version.slug,
											name,
										}),
									back,
								)
							}
							onCancel={back}
						/>
					</div>
				) : (
					<div key={version.slug} className="version__row">
						<MenuItem
							checked={version.current}
							onSelect={() => {
								if (version.current) {
									close();
									return;
								}
								pick(version.slug, false);
							}}
						>
							<span className="menu__tick">
								{version.current && <Icon name="check" />}
							</span>
							{version.name}
						</MenuItem>
						{(!version.fixed || !version.current) && (
							<span className="version__row-actions">
								{!version.current && (
									<button
										type="button"
										className="menu__trigger"
										aria-label={`Compare with ${version.name}`}
										title="Compare with…"
										onClick={() => void compare(version)}
									>
										<Icon name="arrow-left-right" />
									</button>
								)}
								{!version.fixed && (
									<button
										type="button"
										className="menu__trigger"
										aria-label={`Rename ${version.name}`}
										title="Rename"
										onClick={() =>
											setDoing({
												kind: "renaming",
												slug: version.slug,
											})
										}
									>
										<Icon name="pencil" />
									</button>
								)}
								{!version.current && !version.fixed && (
									<button
										type="button"
										className="menu__trigger"
										aria-label={`Delete ${version.name}`}
										title="Delete"
										onClick={() =>
											version.unmerged > 0
												? setDoing({
														kind: "deleting",
														version,
													})
												: void remove(version.slug)
										}
									>
										<Icon name="trash" />
									</button>
								)}
							</span>
						)}
					</div>
				),
			)}

			<span className="menu__rule" />

			{doing.kind === "naming" ? (
				<div className="version__field">
					<NameField
						label="Name of the new version"
						placeholder="Name the new version"
						busy={busy}
						working="Starting…"
						error={error}
						onSubmit={(name) =>
							void named(
								() => invoke("create_branch", { root, name }),
								close,
							)
						}
						onCancel={back}
					/>
				</div>
			) : (
				<>
					<MenuItem
						disabled={kept !== true}
						onSelect={() => setDoing({ kind: "naming" })}
					>
						New version…
					</MenuItem>
					{kept === false && (
						<p className="version__note">
							Add a first checkpoint to start another version.
						</p>
					)}
					<MenuItem
						disabled={others.length === 0 || merging}
						onSelect={() => setDoing({ kind: "picking" })}
					>
						Merge into {current.name}…
					</MenuItem>
					{merging && (
						<p className="version__note">
							Settle or abort the merge under way first.
						</p>
					)}
				</>
			)}
		</>
	);
}
