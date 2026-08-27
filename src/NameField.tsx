import { useState } from "react";

type Props = {
	label: string;
	placeholder: string;
	busy: boolean;
	// Whatever Rust said about the last name that was tried.
	error: string | null;
	onSubmit: (name: string) => void;
	onCancel: () => void;
};

export default function NameField({
	label,
	placeholder,
	busy,
	error,
	onSubmit,
	onCancel,
}: Props) {
	const [name, setName] = useState("");

	return (
		<form
			className="namefield"
			onSubmit={(event) => {
				event.preventDefault();
				// Names are judged in Rust; this only avoids a round trip for
				// a field nobody has typed in.
				if (!busy && name.trim() !== "") {
					onSubmit(name);
				}
			}}
			onBlur={(event) => {
				// Clicking away abandons it. Escape does the same, but a
				// writer reaching for the mouse cannot see Escape.
				const moved = event.relatedTarget;
				const inside =
					moved instanceof Node &&
					event.currentTarget.contains(moved);
				// Not while a name is with Rust: closing now would only reopen
				// the field if that name comes back refused.
				if (!busy && !inside) {
					onCancel();
				}
			}}
		>
			{/* The field only appears because the writer asked for it, so
			    it takes the caret with it. */}
			<input
				className="namefield__input"
				autoFocus
				value={name}
				aria-label={label}
				placeholder={placeholder}
				onChange={(event) => setName(event.target.value)}
				onKeyDown={(event) => {
					if (event.key === "Escape") {
						event.preventDefault();
						onCancel();
					}
				}}
			/>
			<p className="namefield__message" role="status">
				{busy ? "Creating…" : (error ?? "")}
			</p>
		</form>
	);
}
