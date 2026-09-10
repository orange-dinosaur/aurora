import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import Cover from "./Cover";
import Export from "./Export";
import { Chips, TextBox } from "./Field";
import Icon from "./Icon";
import Menu, { MenuItem } from "./Menu";
import { ROLES, added, changed, kept, removed, titled } from "./contributors";
import { failure } from "./errors";
import { GROUPS, filled, known, toggled } from "./groups";
import type { GroupKey } from "./groups";
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
	const [shut, setShut] = useState<GroupKey[]>([]);
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

		invoke<string[]>("read_folded", { root })
			.then((saved) => {
				if (!gone) {
					setShut(known(saved));
				}
			})
			.catch(() => {
				// A page that opens with every group showing is no worse than
				// one that remembered, and not worth a complaint on screen.
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

	/** Writes now whatever is still waiting on the countdown. */
	async function flush() {
		if (timer.current !== null) {
			window.clearTimeout(timer.current);
		}
		await write();
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

	function fold(key: GroupKey) {
		const next = toggled(shut, key);
		setShut(next);
		void invoke("write_folded", { root, shut: next }).catch(() => {
			// The group still folded; only the remembering failed.
		});
	}

	const languages = offered(navigator.language);
	const language = book === null ? "" : named(book.language);

	function group(key: GroupKey, children: ReactNode) {
		const heading = GROUPS.find((each) => each.key === key);

		return (
			<Group
				name={heading === undefined ? key : heading.name}
				count={book === null ? 0 : filled(book, key)}
				open={!shut.includes(key)}
				onToggle={() => fold(key)}
			>
				{children}
			</Group>
		);
	}

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
				<div className="book__fields">
					{group(
						"identity",
						<>
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
								onChange={(subtitle) =>
									edit({ ...book, subtitle })
								}
							/>

							<div className="book__pair book__pair--tight">
								<TextBox
									label="Series"
									value={book.series}
									placeholder="What it belongs to, if anything"
									rows={1}
									onChange={(series) =>
										edit({ ...book, series })
									}
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
										language === ""
											? "Choose a language"
											: language
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
						</>,
					)}

					{group(
						"people",
						<>
							<TextBox
								label="Author"
								value={book.author}
								placeholder="The name on the cover"
								rows={1}
								onChange={(author) => edit({ ...book, author })}
							/>

							<div className="field">
								<span className="field__label">
									Contributors
								</span>
								{book.contributors.length > 0 && (
									<ul className="people">
										{/* Keyed by place, which is what a
										    contributor is held by: two rows can
										    be blank, or the same name in two
										    roles. */}
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
																	name: event
																		.target
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
									onClick={() =>
										people(added(book.contributors))
									}
								>
									<Icon name="plus" />
									Add someone
								</button>
							</div>
						</>,
					)}

					{group(
						"itself",
						<>
							<TextBox
								label="Blurb"
								value={book.blurb}
								placeholder="What the back cover says"
								rows={5}
								onChange={(blurb) => edit({ ...book, blurb })}
							/>
							<Chips
								label="Keywords"
								hint="What a shop would file it under. Enter adds one."
								placeholder="Add a keyword"
								values={book.keywords}
								onChange={(keywords) =>
									edit({ ...book, keywords })
								}
							/>
						</>,
					)}

					{group(
						"publication",
						<>
							<TextBox
								label="Publisher"
								value={book.publisher}
								placeholder="Who is putting it out"
								rows={1}
								onChange={(publisher) =>
									edit({ ...book, publisher })
								}
							/>

							<div className="book__pair">
								<TextBox
									label="Published"
									value={book.publicationDate}
									placeholder="2026, or 2026-09-06"
									rows={1}
									onChange={(publicationDate) =>
										edit({ ...book, publicationDate })
									}
								/>
								<TextBox
									label="ISBN"
									value={book.isbn}
									placeholder="978-0-000-00000-0"
									rows={1}
									onChange={(isbn) => edit({ ...book, isbn })}
								/>
							</div>

							<TextBox
								label="Copyright"
								value={book.copyright}
								placeholder="© 2026 Your name. All rights reserved."
								rows={3}
								onChange={(copyright) =>
									edit({ ...book, copyright })
								}
							/>
						</>,
					)}

					{group(
						"export",
						<>
							<Cover
								root={root}
								cover={book.cover}
								onChange={(cover) => edit({ ...book, cover })}
							/>
							<Export
								root={root}
								hasCover={book.cover !== ""}
								formats={book.exportFormats}
								onFormats={(exportFormats) =>
									edit({ ...book, exportFormats })
								}
								onBeforeExport={flush}
							/>
						</>,
					)}
				</div>
			)}
		</div>
	);
}

type GroupProps = {
	name: string;
	/** How much the group holds, said on it while it is folded. */
	count: number;
	open: boolean;
	onToggle: () => void;
	children: ReactNode;
};

/**
 * One heading's worth of the page. A folded group carries its count so that
 * something filled in does not vanish without a trace when it is shut.
 */
function Group({ name, count, open, onToggle, children }: GroupProps) {
	return (
		<section className="group">
			<button
				type="button"
				className="group__head"
				aria-expanded={open}
				onClick={onToggle}
			>
				<Icon name="chevron-down" className="group__chevron" />
				{name}
				{!open && count > 0 && (
					<span className="group__count">{count}</span>
				)}
			</button>
			{open && <div className="group__body">{children}</div>}
		</section>
	);
}
