import { useState } from "react";

// The controls the panel is built from. A field's text is held here while it is
// being typed and pushed to the document on every keystroke, rather than being
// read straight back out of the editor: the round trip through Lexical and the
// front matter is fast, but not fast enough to be in the way of typing.

type Props = {
	label: string;
	value: string;
	placeholder: string;
	rows: number;
	onChange: (value: string) => void;
};

export default function TextBox({
	label,
	value,
	placeholder,
	rows,
	onChange,
}: Props) {
	const [draft, setDraft] = useState(value);
	const [shown, setShown] = useState(value);

	// Opening another document, or an edit from anywhere but this box, replaces
	// what is being typed. Comparing against the last value seen is what tells
	// the two apart from a keystroke of our own.
	if (value !== shown) {
		setShown(value);
		setDraft(value);
	}

	return (
		<label className="field">
			<span className="field__label">{label}</span>
			<textarea
				className="field__box"
				rows={rows}
				value={draft}
				placeholder={placeholder}
				onChange={(event) => {
					setDraft(event.target.value);
					setShown(event.target.value);
					onChange(event.target.value);
				}}
			/>
		</label>
	);
}
