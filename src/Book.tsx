import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { TextBox } from "./Field";
import Icon from "./Icon";
import Menu, { MenuItem } from "./Menu";
import { ROLES, added, changed, kept, removed, titled } from "./contributors";
import { failure } from "./errors";
import { named, offered } from "./languages";
import type { Book, Contributor } from "./types";

type Props = {
	root: string;
};

/** How long the writer has to stop typing before the book is written. */
const AUTOSAVE_MS = 800;

/**
 * What the project's one book says about itself. The page holds the whole
 * record while it is being edited and sends the whole of it back, because that
 * is the shape `write_book` takes and there is only ever one of these open.
 */
export default function BookView({ root }: Props) {
	const [book, setBook] = useState<Book | null>(null);
	const [trouble, setTrouble] = useState("");
	// What is waiting to be written, and the countdown to writing it. Both are
	// refs so that the flush on the way out sees the last keystroke rather than
	// the state a closed-over render had.
	const pending = useRef<Book | null>(null);
	const timer = useRef<number | null>(null);
	const where = useRef(root);
	where.current = root;

	useEffect(() => {
		let gone = false;

		invoke<Book>("read_book", { root })
			.then((read) => {
				if (!gone) {
					setBook(read);
				}
			})
			.catch((error: unknown) => {
				if (!gone) {
					setTrouble(failure(error).message);
				}
			});

		return () => {
			gone = true;
		};
	}, [root]);

	// Closing the tab is not a reason to lose the last thing typed into it.
	useEffect(
		() => () => {
			if (timer.current !== null) {
				window.clearTimeout(timer.current);
				void write();
			}
		},
		[],
	);

	async function write() {
		const next = pending.current;
		timer.current = null;
		pending.current = null;

		if (next === null) {
			return;
		}

		try {
			await invoke("write_book", {
				root: where.current,
				// A row the writer opened and never named is not a contributor.
				book: { ...next, contributors: kept(next.contributors) },
			});
			setTrouble("");
		} catch (error) {
			setTrouble(failure(error).message);
		}
	}

	function edit(next: Book) {
		setBook(next);
		pending.current = next;

		if (timer.current !== null) {
			window.clearTimeout(timer.current);
		}
		timer.current = window.setTimeout(() => void write(), AUTOSAVE_MS);
	}

	function people(contributors: Contributor[]) {
		if (book !== null) {
			edit({ ...book, contributors });
		}
	}

	const languages = offered(navigator.language);
	const language = book === null ? "" : named(book.language);

	return (
		<div className="book">
			<header className="book__head">
				<p className="book__where">Book</p>
				<h2 className="book__name">
					{book === null || book.title.trim() === ""
						? "Untitled"
						: book.title}
				</h2>
			</header>

			{/* Always drawn, so a complaint about a failed write does not push
			    the field that caused it down the page. */}
			<p className="book__trouble" role="alert">
				{trouble}
			</p>

			{book !== null && (
				<div className="book__group">
					<TextBox
						label="Title"
						value={book.title}
						placeholder="What the book is called"
						rows={1}
						onChange={(title) => edit({ ...book, title })}
					/>
					<TextBox
						label="Subtitle"
						value={book.subtitle}
						placeholder="The line under the title, if there is one"
						rows={1}
						onChange={(subtitle) => edit({ ...book, subtitle })}
					/>
					<TextBox
						label="Author"
						value={book.author}
						placeholder="The name on the cover"
						rows={1}
						onChange={(author) => edit({ ...book, author })}
					/>

					<div className="field">
						<span className="field__label">Contributors</span>
						{book.contributors.length > 0 && (
							<ul className="people">
								{/* Keyed by place, which is what a contributor
								    is held by: two rows can be blank, or the
								    same name in two roles. */}
								{book.contributors.map((person, at) => (
									<li key={at} className="person">
										<input
											className="field__line"
											type="text"
											value={person.name}
											placeholder="Their name"
											aria-label={`Contributor ${at + 1}`}
											onChange={(event) =>
												people(
													changed(
														book.contributors,
														at,
														{
															...person,
															name: event.target
																.value,
														},
													),
												)
											}
										/>
										<Menu
											label={`Role of contributor ${at + 1}`}
											icon="chevron-down"
											text={titled(person.role)}
											trailing
											wrapper="book__picker book__picker--role"
											className="book__pick"
										>
											{(close) =>
												ROLES.map((role) => (
													<MenuItem
														key={role}
														onSelect={() => {
															close();
															people(
																changed(
																	book.contributors,
																	at,
																	{
																		...person,
																		role,
																	},
																),
															);
														}}
													>
														{titled(role)}
													</MenuItem>
												))
											}
										</Menu>
										<button
											type="button"
											className="field__remove"
											aria-label={`Remove contributor ${at + 1}`}
											onClick={() =>
												people(
													removed(
														book.contributors,
														at,
													),
												)
											}
										>
											<Icon name="x" />
										</button>
									</li>
								))}
							</ul>
						)}
						<button
							type="button"
							className="book__add"
							onClick={() => people(added(book.contributors))}
						>
							<Icon name="plus" />
							Add someone
						</button>
					</div>

					<div className="book__pair">
						<TextBox
							label="Series"
							value={book.series}
							placeholder="What it belongs to, if anything"
							rows={1}
							onChange={(series) => edit({ ...book, series })}
						/>
						<TextBox
							label="Number"
							value={book.seriesNumber}
							placeholder="1"
							rows={1}
							onChange={(seriesNumber) =>
								edit({ ...book, seriesNumber })
							}
						/>
					</div>

					<div className="field">
						<span className="field__label">Language</span>
						<Menu
							label="Language"
							icon="chevron-down"
							text={
								language === "" ? "Choose a language" : language
							}
							trailing
							wrapper="book__picker"
							className={
								language === ""
									? "book__pick book__pick--empty"
									: "book__pick"
							}
						>
							{(close) =>
								languages.map((offer) => (
									<MenuItem
										key={offer.tag}
										onSelect={() => {
											close();
											edit({
												...book,
												language: offer.tag,
											});
										}}
									>
										{offer.name}
									</MenuItem>
								))
							}
						</Menu>
					</div>
				</div>
			)}
		</div>
	);
}
