import { useEffect, useRef, useState } from "react";
import { MERGE_HOLDS } from "./branches";

type Props = {
	words: number;
	target: number | null;
	onTarget: (target: number | null) => void;
	merging: boolean;
};

/** The words half of the status line: how many there are, and how many are wanted. */
export function reading(words: number, target: number | null): string {
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
export default function Target({ words, target, onTarget, merging }: Props) {
	const [typing, setTyping] = useState(false);

	if (!typing) {
		return (
			<button
				type="button"
				className="editor__target"
				disabled={merging}
				title={
					merging
						? MERGE_HOLDS
						: target === null
							? "Set a word target"
							: "Change or clear the word target"
				}
				onClick={() => setTyping(true)}
			>
				{reading(words, target)}
			</button>
		);
	}

	return (
		<TargetField
			target={target}
			onTarget={onTarget}
			onDone={() => setTyping(false)}
		/>
	);
}

type FieldProps = {
	target: number | null;
	onTarget: (target: number | null) => void;
	/** Called once the field is finished with, saved or abandoned. */
	onDone: () => void;
	/** For a binder row: no note beside it, which a row has no room for. */
	compact?: boolean;
};

/** The field a target is typed into, wherever a count opens one. */
export function TargetField({
	target,
	onTarget,
	onDone,
	compact = false,
}: FieldProps) {
	const [draft, setDraft] = useState(target === null ? "" : String(target));
	const input = useRef<HTMLInputElement>(null);

	// The field opens on whatever the target is now, ready to be typed over
	// rather than edited a digit at a time.
	useEffect(() => {
		input.current?.select();
	}, []);

	return (
		<form
			className={compact ? "editor__aim row__aim" : "editor__aim"}
			onSubmit={(event) => {
				event.preventDefault();
				// An empty field is how a target is taken away; Rust reads a
				// zero the same way.
				onTarget(draft === "" ? null : Number(draft));
				onDone();
			}}
			onBlur={(event) => {
				// Clicking away abandons it, the same as Escape, which a
				// writer reaching for the mouse cannot see.
				const moved = event.relatedTarget;
				const inside =
					moved instanceof Node &&
					event.currentTarget.contains(moved);
				if (!inside) {
					onDone();
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
				placeholder={compact ? "no target" : "none"}
				// Digits only: a target is a count, and refusing the rest here
				// saves explaining it afterwards.
				onChange={(event) =>
					setDraft(event.target.value.replace(/\D/g, ""))
				}
				onKeyDown={(event) => {
					if (event.key === "Escape") {
						event.preventDefault();
						onDone();
					}
				}}
			/>
			{!compact && (
				<span className="editor__aim-note">
					{draft === "" ? "no target" : "word target"}
				</span>
			)}
		</form>
	);
}
