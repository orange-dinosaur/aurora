import { useId, useState } from "react";
import type { KeyboardEvent, ReactNode } from "react";
import Icon from "./Icon";
import { MERGE_HOLDS } from "./branches";
import type { Ties } from "./frontmatter";

// The controls the panel is built from. A field's text is held here while it is
// being typed and pushed to the document on every keystroke, rather than being
// read straight back out of the editor: the round trip through Lexical and the
// front matter is fast, but not fast enough to be in the way of typing.

/**
 * Stills every field inside it while a merge waits, since settling the merge
 * writes the book and its folders afresh and would drop anything typed here.
 */
export function Still({
	merging,
	children,
}: {
	merging: boolean;
	children: ReactNode;
}) {
	return (
		<fieldset
			className="still"
			disabled={merging}
			title={merging ? MERGE_HOLDS : undefined}
		>
			{children}
		</fieldset>
	);
}

type BoxProps = {
	label: string;
	value: string;
	placeholder: string;
	/** One row makes it a single line rather than a box. */
	rows: number;
	/** Set in the page font, for prose rather than a detail. */
	page?: boolean;
	onChange: (value: string) => void;
	/** Given only for a field the writer added, which they can take away. */
	onRemove?: () => void;
};

export function TextBox({
	label,
	value,
	placeholder,
	rows,
	page = false,
	onChange,
	onRemove,
}: BoxProps) {
	const id = useId();
	const [draft, setDraft] = useState(value);
	const [shown, setShown] = useState(value);

	// Opening another document, or an edit from anywhere but this box, replaces
	// what is being typed. Comparing against the last value seen is what tells
	// the two apart from a keystroke of our own.
	if (value !== shown) {
		setShown(value);
		setDraft(value);
	}

	function typed(next: string) {
		setDraft(next);
		setShown(next);
		onChange(next);
	}

	return (
		<div className="field">
			<div className="field__head">
				<label className="field__label" htmlFor={id}>
					{label}
				</label>
				{onRemove !== undefined && (
					<button
						type="button"
						className="field__remove"
						aria-label={`Remove the field ${label}`}
						onClick={onRemove}
					>
						<Icon name="x" />
					</button>
				)}
			</div>
			{rows === 1 ? (
				<input
					id={id}
					className="field__line"
					type="text"
					value={draft}
					placeholder={placeholder}
					onChange={(event) => typed(event.target.value)}
				/>
			) : (
				<textarea
					id={id}
					className={
						page ? "field__box field__box--page" : "field__box"
					}
					rows={rows}
					value={draft}
					placeholder={placeholder}
					onChange={(event) => typed(event.target.value)}
				/>
			)}
		</div>
	);
}

type ChipsProps = {
	label: string;
	/** A line under the label, for a rule the writer cannot guess. */
	hint?: string;
	placeholder: string;
	values: string[];
	onChange: (values: string[]) => void;
	/** Given when a chip goes somewhere, which turns its label into a link. */
	onOpen?: (value: string) => void;
};

/**
 * A list of short values, entered one at a time rather than as a line of commas
 * the writer has to punctuate.
 */
export function Chips({
	label,
	hint,
	placeholder,
	values,
	onChange,
	onOpen,
}: ChipsProps) {
	const [draft, setDraft] = useState("");

	function add() {
		const value = draft.trim();

		// One already there is not added twice, and neither is nothing.
		if (value !== "" && !values.includes(value)) {
			onChange([...values, value]);
		}
		setDraft("");
	}

	function remove(value: string) {
		onChange(values.filter((each) => each !== value));
	}

	return (
		<div className="field">
			<span className="field__label">{label}</span>
			{hint !== undefined && <span className="field__hint">{hint}</span>}
			<ul className="chips">
				{values.map((value) => (
					<li key={value} className="chip">
						{onOpen === undefined ? (
							value
						) : (
							<button
								type="button"
								className="chip__open"
								onClick={() => onOpen(value)}
							>
								{value}
							</button>
						)}
						<button
							type="button"
							className="chip__remove"
							aria-label={`Remove ${value}`}
							onClick={() => remove(value)}
						>
							<Icon name="x" />
						</button>
					</li>
				))}
				<li className="chips__entry">
					<input
						className="chips__input"
						type="text"
						value={draft}
						placeholder={placeholder}
						aria-label={placeholder}
						onChange={(event) => setDraft(event.target.value)}
						onKeyDown={(event) => {
							if (event.key === "Enter") {
								event.preventDefault();
								add();
							} else if (event.key === "Escape") {
								event.preventDefault();
								setDraft("");
							} else if (
								event.key === "Backspace" &&
								draft === "" &&
								values.length > 0
							) {
								remove(values[values.length - 1]);
							}
						}}
						// Clicking away abandons what was half typed, the way
						// every other field that appears in place behaves.
						onBlur={() => setDraft("")}
					/>
				</li>
			</ul>
		</div>
	);
}

type Suggesting = ReturnType<typeof useSuggestions>;

/**
 * The names a box under a field offers while it is focused, and which of them
 * the arrow keys have marked. The box keeps its own text and says what to do
 * with a name that is picked.
 */
function useSuggestions(
	names: string[],
	draft: string,
	onAsk: () => void,
	pick: (name: string) => void,
) {
	const id = useId();
	const [open, setOpen] = useState(false);
	const [marked, setMarked] = useState(-1);

	const wanted = draft.trim().toLowerCase();
	const offered = names
		.filter((name) => name.toLowerCase().includes(wanted))
		.slice(0, 6);
	const showing = open && offered.length > 0;
	const at = showing && marked < offered.length ? marked : -1;

	function choose(name: string) {
		setMarked(-1);
		pick(name);
	}

	return {
		id,
		offered,
		showing,
		at,
		choose,
		/** The attributes that tie the box to its list. */
		aria: {
			role: "combobox",
			"aria-expanded": showing,
			"aria-controls": showing ? id : undefined,
			"aria-autocomplete": "list",
			"aria-activedescendant": at < 0 ? undefined : `${id}-${at}`,
		} as const,
		focus() {
			setOpen(true);
			onAsk();
		},
		blur() {
			setOpen(false);
			setMarked(-1);
		},
		typed() {
			setMarked(-1);
		},
		/** Answers the arrows and Enter on a marked name; true when it did. */
		key(event: KeyboardEvent<HTMLInputElement>): boolean {
			if (!showing) {
				return false;
			}
			if (event.key === "ArrowDown" || event.key === "ArrowUp") {
				event.preventDefault();
				// One stop past each end is no name marked, so the writer can
				// get back to the text they typed.
				const stops = offered.length + 1;
				const step = event.key === "ArrowDown" ? 1 : -1;
				setMarked(((at + 1 + step + stops) % stops) - 1);
				return true;
			}
			if (event.key === "Enter" && at >= 0) {
				event.preventDefault();
				choose(offered[at]);
				return true;
			}
			return false;
		},
	};
}

function Suggestions({ of }: { of: Suggesting }) {
	if (!of.showing) {
		return null;
	}

	return (
		<ul className="suggestions" id={of.id} role="listbox">
			{of.offered.map((name, index) => (
				<li key={name} role="presentation">
					<button
						type="button"
						role="option"
						id={`${of.id}-${index}`}
						aria-selected={index === of.at}
						className="suggestion"
						// Taken out of the tab order: the arrows move through
						// the list while focus stays in the box.
						tabIndex={-1}
						// Mouse down rather than click: the blur that closes the
						// list would otherwise happen first and take the button
						// with it.
						onMouseDown={(event) => {
							event.preventDefault();
							of.choose(name);
						}}
					>
						{name}
					</button>
				</li>
			))}
		</ul>
	);
}

type TieProps = {
	label: string;
	hint?: string;
	placeholder: string;
	values: Ties;
	/** The subjects this project has, for the box to suggest. */
	names: string[];
	/** Read those subjects, which is put off until the box is used. */
	onAsk: () => void;
	onChange: (next: Ties) => void;
};

/**
 * Names paired with a line each: who this page is tied to, and what the tie is.
 * A name is added or dropped whole and is not edited in place, since it is the
 * key the note hangs off; rewording a tie is the note's job and renaming one is
 * a remove and an add.
 */
export function TieList({
	label,
	hint,
	placeholder,
	values,
	names,
	onAsk,
	onChange,
}: TieProps) {
	const [draft, setDraft] = useState("");

	function add(name: string) {
		const subject = name.trim();

		// One already here is not added twice, and neither is nothing.
		if (subject !== "" && !values.has(subject)) {
			onChange(new Map([...values, [subject, ""]]));
		}
		setDraft("");
	}

	const suggesting = useSuggestions(
		names.filter((name) => !values.has(name)),
		draft,
		onAsk,
		add,
	);

	function word(subject: string, said: string) {
		onChange(
			new Map(
				[...values].map(([name, was]) => [
					name,
					name === subject ? said : was,
				]),
			),
		);
	}

	function remove(subject: string) {
		onChange(new Map([...values].filter(([name]) => name !== subject)));
	}

	return (
		<div className="field">
			<span className="field__label">{label}</span>
			{hint !== undefined && <span className="field__hint">{hint}</span>}

			{values.size > 0 && (
				<ul className="ties">
					{[...values].map(([name, said]) => (
						<li key={name} className="tie">
							<span className="tie__who">
								<span className="tie__name">{name}</span>
								<button
									type="button"
									className="chip__remove"
									aria-label={`Remove ${name}`}
									onClick={() => remove(name)}
								>
									<Icon name="x" />
								</button>
							</span>
							<input
								className="field__line"
								type="text"
								value={said}
								placeholder="How they are tied"
								aria-label={`How ${name} is tied`}
								onChange={(event) =>
									word(name, event.target.value)
								}
							/>
						</li>
					))}
				</ul>
			)}

			<input
				className="field__line"
				type="text"
				value={draft}
				placeholder={placeholder}
				aria-label={placeholder}
				{...suggesting.aria}
				onFocus={suggesting.focus}
				onChange={(event) => {
					setDraft(event.target.value);
					suggesting.typed();
				}}
				onKeyDown={(event) => {
					if (suggesting.key(event)) {
						return;
					}
					if (event.key === "Enter") {
						event.preventDefault();
						add(draft);
					} else if (event.key === "Escape") {
						event.preventDefault();
						setDraft("");
					}
				}}
				// Clicking away abandons what was half typed, the way every
				// other field that appears in place behaves.
				onBlur={() => {
					suggesting.blur();
					setDraft("");
				}}
			/>

			<Suggestions of={suggesting} />
		</div>
	);
}

type AddProps = {
	/** Names used elsewhere in the project, minus the ones already here. */
	names: string[];
	/** Read those names, which is put off until the box is used. */
	onAsk: () => void;
	/** Null when the field was added, otherwise why it was not. */
	onAdd: (name: string) => string | null;
};

/** The box that gives a document a field of the writer's own. */
export function AddField({ names, onAsk, onAdd }: AddProps) {
	const [draft, setDraft] = useState("");
	// Why the last name was refused. The line is always there, so saying it
	// moves nothing.
	const [refused, setRefused] = useState<string | null>(null);

	function add(name: string) {
		if (name.trim() !== "") {
			const reason = onAdd(name.trim());
			if (reason !== null) {
				setRefused(reason);
				return;
			}
		}
		setRefused(null);
		setDraft("");
	}

	const suggesting = useSuggestions(names, draft, onAsk, add);

	return (
		<div className="field">
			<span className="field__label">Add field</span>
			<input
				className="field__line"
				type="text"
				value={draft}
				placeholder="Field name"
				aria-label="Add a field"
				{...suggesting.aria}
				onFocus={suggesting.focus}
				onChange={(event) => {
					setDraft(event.target.value);
					setRefused(null);
					suggesting.typed();
				}}
				onKeyDown={(event) => {
					if (suggesting.key(event)) {
						return;
					}
					if (event.key === "Enter") {
						event.preventDefault();
						add(draft);
					} else if (event.key === "Escape") {
						event.preventDefault();
						setDraft("");
						setRefused(null);
					}
				}}
				onBlur={() => {
					suggesting.blur();
					setDraft("");
					setRefused(null);
				}}
			/>
			<span className="field__hint field__hint--held" role="status">
				{refused}
			</span>
			<Suggestions of={suggesting} />
		</div>
	);
}
