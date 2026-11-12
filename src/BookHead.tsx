import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { useCoverImage } from "./Cover";
import Icon from "./Icon";
import Menu, { MenuItem } from "./Menu";
import VersionMenu from "./VersionMenu";
import { MERGE_HOLDS } from "./branches";
import { failure } from "./errors";
import { coverFor } from "./lib/covers";
import type { Book, Branch } from "./types";

type BookProps = {
	root: string;
	/** The project's name, which stands in until the book has a title. */
	name: string;
	/** Bumped when the project changes on disk. */
	reload: number;
	/** Bumped when Book details has written the book. */
	written: number;
	/** Whether a merge is waiting to be settled. */
	merging: boolean;
	onOpenBook: () => void;
	onClose: () => void;
	/** The manifest was read again, so every listing of it should be too. */
	onChanged: () => void;
	/** What went wrong, or null once it no longer applies. */
	onMessage: (message: string | null) => void;
};

type Props = BookProps & {
	/** Opens compare at a checkpoint, listing the given version's history. */
	onCompare: (checkpoint: string, slug: string) => void;
	/** Writes every open tab, so asking what is not in a checkpoint sees it. */
	onBeforeSwitch: () => Promise<void>;
	/** Runs a switch the way put back runs, catching the tabs up after it. */
	onSwitch: (task: () => Promise<unknown>) => Promise<void>;
	/** Reads the merge again after a switch or a merge from here. */
	onMergeChanged: () => Promise<void>;
};

/**
 * What leads the binder: the book's cover and title, which open what can be
 * done to the project as a whole, and the version it is on.
 */
export default function BookHead({
	onCompare,
	onBeforeSwitch,
	onSwitch,
	onMergeChanged,
	...book
}: Props) {
	const { root, merging, onMessage } = book;
	const [versions, setVersions] = useState<Branch[]>([]);
	const current = versions.find((version) => version.current);

	async function listVersions() {
		try {
			setVersions(await invoke<Branch[]>("list_branches", { root }));
		} catch (error) {
			onMessage(failure(error).message);
		}
	}

	useEffect(() => {
		void listVersions();
	}, [root]);

	// Whether there are changes the writer has to decide about first. With
	// none, the switch or the merge is already under way.
	async function choose(slug: string, merge: boolean): Promise<boolean> {
		await onBeforeSwitch();
		try {
			if (await invoke<boolean>("changes_pending", { root })) {
				return true;
			}
		} catch (error) {
			onMessage(failure(error).message);
			return false;
		}
		void switchTo(slug, null, merge);
		return false;
	}

	// Merging rewrites files the way switching does, so it catches the tabs
	// up the same way.
	async function switchTo(
		slug: string,
		checkpoint: string | null,
		merge: boolean,
	) {
		try {
			await onSwitch(async () => {
				if (checkpoint !== null) {
					await invoke("add_checkpoint", {
						root,
						message: checkpoint,
					});
				}
				await invoke(merge ? "merge_branch" : "switch_branch", {
					root,
					slug,
				});
			});
			onMessage(null);
		} catch (error) {
			onMessage(failure(error).message);
		}
		await onMergeChanged();
		await listVersions();
	}

	return (
		<div className="binder__head">
			<BookMenu {...book} />

			{current !== undefined && (
				<Menu
					label={`Version: ${current.name}`}
					trigger={
						<>
							<span className="binder__version-name">
								{current.name}
							</span>
							<Icon name="chevron-down" />
						</>
					}
					wrapper="binder__menu binder__menu--version"
					className="binder__version"
				>
					{(close) => (
						<VersionMenu
							root={root}
							versions={versions}
							current={current}
							merging={merging}
							close={close}
							onChoose={choose}
							onCheckpoint={(slug, words, merge) =>
								void switchTo(slug, words, merge)
							}
							onCompare={onCompare}
							onChanged={listVersions}
							onFailure={onMessage}
						/>
					)}
				</Menu>
			)}
		</div>
	);
}

/**
 * The book's cover and title, which open what can be done to the project as a
 * whole. Compare and Merge lead their lists with it too.
 */
export function BookMenu({
	root,
	name,
	reload,
	written,
	merging,
	onOpenBook,
	onClose,
	onChanged,
	onMessage,
}: BookProps) {
	const [book, setBook] = useState<Book | null>(null);
	const [busy, setBusy] = useState(false);
	const { src } = useCoverImage(root, book?.cover ?? "", reload + written);
	const title = book === null || book.title.trim() === "" ? name : book.title;

	// A book that cannot be read costs the head its details, not the project.
	useEffect(() => {
		let listening = true;
		invoke<Book>("read_book", { root })
			.then((read) => {
				if (listening) {
					setBook(read);
				}
			})
			.catch(() => {
				if (listening) {
					setBook(null);
				}
			});

		return () => {
			listening = false;
		};
	}, [root, reload, written]);

	// Hands the project folder to the desktop's file manager. Nothing about
	// the project changes, so a refusal is worth saying once and no more.
	async function reveal() {
		try {
			await revealItemInDir(root);
			onMessage(null);
		} catch (error) {
			onMessage(failure(error).message);
		}
	}

	// `refresh_documents` looks at the folder again before rewriting the
	// manifest, for anything that changed outside Aurora.
	async function refresh() {
		setBusy(true);
		try {
			await invoke("refresh_documents", { root });
			onMessage(null);
			onChanged();
		} catch (error) {
			onMessage(failure(error).message);
		} finally {
			setBusy(false);
		}
	}

	return (
		<Menu
			label={`${title}: book menu`}
			wrapper="binder__menu"
			className="binder__book"
			trigger={
				<>
					{/* A wash of the title stands behind an image too, so
					    the cover is never blank while one loads. */}
					<span
						className="binder__cover"
						style={{ background: coverFor(title) }}
						aria-hidden="true"
					>
						{src !== "" && <img src={src} alt="" />}
					</span>
					<span className="binder__names">
						<span className="binder__title">{title}</span>
						{book !== null && book.author.trim() !== "" && (
							<span className="binder__author">
								{book.author}
							</span>
						)}
					</span>
				</>
			}
		>
			{(close) => (
				<>
					<MenuItem
						disabled={merging}
						title={merging ? MERGE_HOLDS : undefined}
						onSelect={() => {
							close();
							onOpenBook();
						}}
					>
						Book details
					</MenuItem>
					<MenuItem
						onSelect={() => {
							close();
							void reveal();
						}}
					>
						Show in Files
						<span className="binder__path" title={root}>
							{root}
						</span>
					</MenuItem>
					<MenuItem
						disabled={merging || busy}
						title={merging ? MERGE_HOLDS : undefined}
						onSelect={() => {
							close();
							void refresh();
						}}
					>
						{busy ? "Refreshing…" : "Refresh documents"}
					</MenuItem>
					<span className="menu__rule" />
					<MenuItem
						onSelect={() => {
							close();
							onClose();
						}}
					>
						Close project
					</MenuItem>
				</>
			)}
		</Menu>
	);
}
