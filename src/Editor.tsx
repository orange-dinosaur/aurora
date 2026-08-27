type Props = {
	title: string;
	text: string;
	dirty: boolean;
	saving: boolean;
	error: string | null;
	onChange: (text: string) => void;
};

export default function Editor({
	title,
	text,
	dirty,
	saving,
	error,
	onChange,
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
					error === null
						? "editor__status"
						: "editor__status editor__status--error"
				}
				role="status"
			>
				{note}
			</p>
		</div>
	);
}
