import { LexicalComposer } from "@lexical/react/LexicalComposer";
import { ContentEditable } from "@lexical/react/LexicalContentEditable";
import { LexicalErrorBoundary } from "@lexical/react/LexicalErrorBoundary";
import { HistoryPlugin } from "@lexical/react/LexicalHistoryPlugin";
import { OnChangePlugin } from "@lexical/react/LexicalOnChangePlugin";
import { RichTextPlugin } from "@lexical/react/LexicalRichTextPlugin";
import { useState } from "react";
import { $fromMarkdown, $toMarkdown, EDITOR_NODES } from "./markdown";

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

	// The editor owns its text from here on, so the document seeds it once and
	// is never pushed in again — doing that on every render would drag the
	// caret out from under whoever is typing.
	const [config] = useState(() => ({
		namespace: "aurora",
		nodes: EDITOR_NODES,
		editorState: () => $fromMarkdown(text),
		onError: (failed: Error) => {
			throw failed;
		},
	}));

	return (
		<div className="editor">
			<h2 className="editor__title">{title}</h2>
			<LexicalComposer initialConfig={config}>
				<div className="editor__surface">
					<RichTextPlugin
						contentEditable={
							<ContentEditable
								className="editor__text"
								aria-label={title}
								aria-placeholder="Start writing…"
								placeholder={
									<p className="editor__placeholder">
										Start writing…
									</p>
								}
								spellCheck
							/>
						}
						ErrorBoundary={LexicalErrorBoundary}
					/>
					<HistoryPlugin />
					{/* Moving the caret is not an edit, or every click would
					    mark the document unsaved. */}
					<OnChangePlugin
						ignoreSelectionChange
						onChange={(state) =>
							onChange(state.read(() => $toMarkdown()))
						}
					/>
				</div>
			</LexicalComposer>
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
