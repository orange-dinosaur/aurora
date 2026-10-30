// The thin line above the text that switches between a document's takes, and
// where each of them is renamed or deleted.

import { useEffect, useRef, useState } from "react";
import type { TakeView } from "./bindings/TakeView";
import { counted } from "./cards";
import { failure } from "./errors";
import Menu, { MenuItem } from "./Menu";
import NameField from "./NameField";

type Renaming = { id: string; busy: boolean; error: string | null };

type Props = {
	takes: TakeView[];
	/** The chosen take's words as they stand, ahead of the list. */
	words: number;
	/** Set while a merge is under way, when takes cannot change. */
	merging: boolean;
	onChoose: (take: string) => void;
	onTry: () => void;
	/** Rejects with what Rust made of the name, which the field shows. */
	onRename: (take: string, name: string) => Promise<void>;
	onDelete: (take: string) => void;
};

export default function TakeStrip({
	takes,
	words,
	merging,
	onChoose,
	onTry,
	onRename,
	onDelete,
}: Props) {
	const [renaming, setRenaming] = useState<Renaming | null>(null);

	async function rename(take: string, name: string) {
		setRenaming({ id: take, busy: true, error: null });
		try {
			await onRename(take, name);
			setRenaming(null);
		} catch (error) {
			setRenaming({
				id: take,
				busy: false,
				error: failure(error).message,
			});
		}
	}

	return (
		<div className="takes">
			<ul className="takes__list" aria-label="Takes">
				{takes.map((take) =>
					renaming?.id === take.id ? (
						<li
							key={take.id}
							className="takes__item takes__item--naming"
						>
							<NameField
								label={`New name for ${take.name}`}
								placeholder={take.name}
								initial={take.name}
								busy={renaming.busy}
								working="Renaming…"
								error={renaming.error}
								onSubmit={(name) => void rename(take.id, name)}
								onCancel={() => setRenaming(null)}
							/>
						</li>
					) : (
						<li
							key={take.id}
							className={
								take.chosen
									? "takes__item takes__item--chosen"
									: "takes__item"
							}
						>
							<button
								type="button"
								className="takes__button"
								aria-current={take.chosen ? "true" : undefined}
								disabled={merging}
								onClick={() => onChoose(take.id)}
							>
								{take.name}
							</button>
							{!merging && (
								<Menu
									label={`Actions for ${take.name}`}
									icon="more-vertical"
									wrapper="takes__menu"
								>
									{(close) => (
										<Actions
											name={take.name}
											words={
												take.chosen ? words : take.words
											}
											close={close}
											onRename={() =>
												setRenaming({
													id: take.id,
													busy: false,
													error: null,
												})
											}
											onDelete={() => onDelete(take.id)}
										/>
									)}
								</Menu>
							)}
						</li>
					),
				)}
			</ul>
			<button
				type="button"
				className="takes__button takes__try"
				disabled={merging}
				onClick={onTry}
			>
				Try something
			</button>
		</div>
	);
}

// Its own component so that its state is the menu's, and a menu left asking
// opens on the actions again next time.
function Actions({
	name,
	words,
	close,
	onRename,
	onDelete,
}: {
	name: string;
	words: number;
	close: () => void;
	onRename: () => void;
	onDelete: () => void;
}) {
	const [asking, setAsking] = useState(false);

	if (asking) {
		return (
			<Asking
				name={name}
				words={words}
				close={close}
				onDelete={onDelete}
			/>
		);
	}

	return (
		<>
			<MenuItem
				onSelect={() => {
					close();
					onRename();
				}}
			>
				Rename
			</MenuItem>
			<MenuItem danger onSelect={() => setAsking(true)}>
				Delete
			</MenuItem>
		</>
	);
}

function Asking({
	name,
	words,
	close,
	onDelete,
}: {
	name: string;
	words: number;
	close: () => void;
	onDelete: () => void;
}) {
	const note = useRef<HTMLParagraphElement>(null);

	// The item that asked has just been unmounted, taking the keyboard with it.
	// The note claims it rather than a button, so Enter deletes nothing.
	useEffect(() => {
		note.current?.focus();
	}, []);

	return (
		<>
			<p ref={note} className="takes__note" tabIndex={-1}>
				{name} · {counted(words, null)}
			</p>
			<MenuItem
				danger
				onSelect={() => {
					close();
					onDelete();
				}}
			>
				Delete for good
			</MenuItem>
			<MenuItem onSelect={close}>Keep</MenuItem>
		</>
	);
}
