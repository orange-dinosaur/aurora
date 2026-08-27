type Props = {
	title: string;
	text: string;
	dirty: boolean;
	saving: boolean;
	missing: boolean;
	error: string | null;
	onChange: (text: string) => void;
	onRestore: () => void;
};

export default function Editor({
	title,
	text,
	dirty,
	saving,
	missing,
	error,
	onChange,
	onRestore,
}: Props) {
	const note = error ?? (saving ? "Saving…" : dirty ? "Unsaved" : "Saved");

	return (
		<div className="editor">
			<h2 className="editor__title">{title}</h2>
			<textarea
				className="editor__text"
				value={text}
				aria-label={title}
				placeholder="Start writing…"
				spellCheck
				onChange={(event) => onChange(event.target.value)}
			/>
			<p
				className={
					error === null && !missing
						? "editor__status"
						: "editor__status editor__status--error"
				}
				role="status"
			>
				{missing ? (
					<>
						This document&rsquo;s file is no longer there.{" "}
						<button
							type="button"
							className="editor__restore"
							onClick={onRestore}
						>
							Write it back
						</button>
					</>
				) : (
					note
				)}
			</p>
		</div>
	);
}
