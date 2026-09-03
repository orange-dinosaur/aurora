import { useId, useState } from "react";
import Icon from "./Icon";

// The controls the panel is built from. A field's text is held here while it is
// being typed and pushed to the document on every keystroke, rather than being
// read straight back out of the editor: the round trip through Lexical and the
// front matter is fast, but not fast enough to be in the way of typing.

type BoxProps = {
	label: string;
	value: string;
	placeholder: string;
	/** One row makes it a single line rather than a box. */
	rows: number;
	onChange: (value: string) => void;
	/** Given only for a field the writer added, which they can take away. */
	onRemove?: () => void;
};

export function TextBox({
	label,
	value,
	placeholder,
	rows,
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
					className="field__box"
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
			{values.length > 0 && (
				<ul className="chips">
					{values.map((value) => (
						<li key={value} className="chip">
							{value}
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
				</ul>
			)}
			<input
				className="field__line"
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
						setDraft("");
					} else if (
						event.key === "Backspace" &&
						draft === "" &&
						values.length > 0
					) {
						remove(values[values.length - 1]);
					}
				}}
				// Clicking away abandons what was half typed, the way every
				// other field that appears in place behaves.
				onBlur={() => setDraft("")}
			/>
		</div>
	);
}

type AddProps = {
	/** Names used elsewhere in the project, minus the ones already here. */
	names: string[];
	/** Read those names, which is put off until the box is used. */
	onAsk: () => void;
	onAdd: (name: string) => void;
};

/** The box that gives a document a field of the writer's own. */
export function AddField({ names, onAsk, onAdd }: AddProps) {
	const [draft, setDraft] = useState("");
	const [open, setOpen] = useState(false);

	const wanted = draft.trim().toLowerCase();
	const offered = names
		.filter((name) => name.toLowerCase().includes(wanted))
		.slice(0, 6);

	function add(name: string) {
		if (name.trim() !== "") {
			onAdd(name.trim());
		}
		setDraft("");
	}

	return (
		<div className="field">
			<span className="field__label">Add field</span>
			<input
				className="field__line"
				type="text"
				value={draft}
				placeholder="Field name"
				aria-label="Add a field"
				onFocus={() => {
					setOpen(true);
					onAsk();
				}}
				onChange={(event) => setDraft(event.target.value)}
				onKeyDown={(event) => {
					if (event.key === "Enter") {
						event.preventDefault();
						add(draft);
					} else if (event.key === "Escape") {
						setDraft("");
					}
				}}
				onBlur={() => {
					setOpen(false);
					setDraft("");
				}}
			/>
			{open && offered.length > 0 && (
				<ul className="suggestions">
					{offered.map((name) => (
						<li key={name}>
							<button
								type="button"
								className="suggestion"
								// Mouse down rather than click: the blur that
								// closes the list would otherwise happen first
								// and take the button with it.
								onMouseDown={(event) => {
									event.preventDefault();
									add(name);
								}}
							>
								{name}
							</button>
						</li>
					))}
				</ul>
			)}
		</div>
	);
}
