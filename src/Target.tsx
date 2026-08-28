import { useEffect, useRef, useState } from "react";

type Props = {
	words: number;
	target: number | null;
	onTarget: (target: number | null) => void;
};

/** The words half of the status line: how many there are, and how many are wanted. */
function reading(words: number, target: number | null): string {
	const counted = words.toLocaleString();
	if (target === null) {
		return words === 1 ? "1 word" : `${counted} words`;
	}

	const share = Math.round((words / target) * 100);
	return `${counted} of ${target.toLocaleString()} words · ${share}%`;
}

/**
 * The word count, which is also how a target is set: there is no button for it
 * anywhere, the number itself is the control.
 */
export default function Target({ words, target, onTarget }: Props) {
	const [typing, setTyping] = useState(false);
	const [draft, setDraft] = useState("");
	const input = useRef<HTMLInputElement>(null);

	// The field opens on whatever the target is now, ready to be typed over
	// rather than edited a digit at a time.
	useEffect(() => {
		if (typing) {
			input.current?.select();
		}
	}, [typing]);

	function open() {
		setDraft(target === null ? "" : String(target));
		setTyping(true);
	}

	if (!typing) {
		return (
			<button
				type="button"
				className="editor__target"
				title={
					target === null
						? "Set a word target"
						: "Change or clear the word target"
				}
				onClick={open}
			>
				{reading(words, target)}
			</button>
		);
	}

	return (
		<form
			className="editor__aim"
			onSubmit={(event) => {
				event.preventDefault();
				// An empty field is how a target is taken away; Rust reads a
				// zero the same way.
				onTarget(draft === "" ? null : Number(draft));
				setTyping(false);
			}}
			onBlur={(event) => {
				// Clicking away abandons it, the same as Escape, which a
				// writer reaching for the mouse cannot see.
				const moved = event.relatedTarget;
				const inside =
					moved instanceof Node &&
					event.currentTarget.contains(moved);
				if (!inside) {
					setTyping(false);
				}
			}}
		>
			<input
				ref={input}
				className="editor__aim-input"
				autoFocus
				value={draft}
				inputMode="numeric"
				aria-label="Word target"
				placeholder="none"
				// Digits only: a target is a count, and refusing the rest here
				// saves explaining it afterwards.
				onChange={(event) =>
					setDraft(event.target.value.replace(/\D/g, ""))
				}
				onKeyDown={(event) => {
					if (event.key === "Escape") {
						event.preventDefault();
						setTyping(false);
					}
				}}
			/>
			<span className="editor__aim-note">
				{draft === "" ? "no target" : "word target"}
			</span>
		</form>
	);
}
