import { useEffect, useRef, useState } from "react";
import { MERGE_HOLDS } from "./branches";

type Props = {
	words: number;
	target: number | null;
	onTarget: (target: number | null) => void;
	merging: boolean;
	/**
	 * Keep the sentence while typing, editing only the target inside it. For
	 * a header line, where the field stands among words rather than alone.
	 */
	inline?: boolean;
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
export default function Target({
	words,
	target,
	onTarget,
	merging,
	inline = false,
}: Props) {
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
			words={inline ? words : undefined}
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
	/** Given, the field stands in a sentence: "11 of [target] words". */
	words?: number;
};

/** The field a target is typed into, wherever a count opens one. */
export function TargetField({
	target,
	onTarget,
	onDone,
	compact = false,
	words,
}: FieldProps) {
	const sentence = words !== undefined;
	const [draft, setDraft] = useState(target === null ? "" : String(target));
	const input = useRef<HTMLInputElement>(null);

	// The field opens on whatever the target is now, ready to be typed over
	// rather than edited a digit at a time.
	useEffect(() => {
		input.current?.select();
	}, []);

	return (
		<form
			className={
				compact
					? "editor__aim row__aim"
					: sentence
						? "editor__aim aim--sentence"
						: "editor__aim"
			}
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
			{sentence && <span>{words.toLocaleString()} of</span>}
			<input
				ref={input}
				className="editor__aim-input"
				autoFocus
				value={draft}
				inputMode="numeric"
				aria-label="Word target"
				placeholder={compact || sentence ? "no target" : "none"}
				// In a sentence the field is as wide as what it holds, so the
				// words after it sit where they would in the line it replaced.
				style={
					sentence
						? {
								width: `${Math.max(draft.length, draft === "" ? 9 : 1) + 1}ch`,
							}
						: undefined
				}
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
			{sentence && <span>words</span>}
			{!compact && !sentence && (
				<span className="editor__aim-note">
					{draft === "" ? "no target" : "word target"}
				</span>
			)}
		</form>
	);
}
