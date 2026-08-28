import { LexicalComposer } from "@lexical/react/LexicalComposer";
import { ContentEditable } from "@lexical/react/LexicalContentEditable";
import { LexicalErrorBoundary } from "@lexical/react/LexicalErrorBoundary";
import { HistoryPlugin } from "@lexical/react/LexicalHistoryPlugin";
import { LinkPlugin } from "@lexical/react/LexicalLinkPlugin";
import { ListPlugin } from "@lexical/react/LexicalListPlugin";
import { MarkdownShortcutPlugin } from "@lexical/react/LexicalMarkdownShortcutPlugin";
import { OnChangePlugin } from "@lexical/react/LexicalOnChangePlugin";
import { RichTextPlugin } from "@lexical/react/LexicalRichTextPlugin";
import type { EditorThemeClasses } from "lexical";
import { useState } from "react";
import {
	$fromMarkdown,
	$toMarkdown,
	EDITOR_NODES,
	MARKDOWN_TRANSFORMERS,
} from "./markdown";
import Links from "./Links";
import Shortcuts from "./Shortcuts";
import Toolbar from "./Toolbar";

// Lexical puts these class names on the elements it renders; App.css styles
// them. Bold and italic are left out because they come out as <strong> and
// <em>, but a strikethrough or an underline has no tag of its own and would be
// invisible without a class.
const THEME: EditorThemeClasses = {
	paragraph: "editor__paragraph",
	heading: {
		h1: "editor__heading editor__heading--1",
		h2: "editor__heading editor__heading--2",
		h3: "editor__heading editor__heading--3",
		h4: "editor__heading editor__heading--4",
		h5: "editor__heading editor__heading--5",
		h6: "editor__heading editor__heading--6",
	},
	quote: "editor__quote",
	list: {
		ul: "editor__list",
		ol: "editor__list",
		listitem: "editor__item",
		nested: { listitem: "editor__item--nested" },
	},
	code: "editor__code-block",
	hr: "editor__rule",
	link: "editor__link",
	text: {
		code: "editor__code",
		highlight: "editor__mark",
		strikethrough: "editor__struck",
		underline: "editor__underlined",
		underlineStrikethrough: "editor__underlined editor__struck",
	},
};

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
		theme: THEME,
		editorState: () => $fromMarkdown(text),
		onError: (failed: Error) => {
			throw failed;
		},
	}));

	return (
		<div className="editor">
			<h2 className="editor__title">{title}</h2>
			<LexicalComposer initialConfig={config}>
				<Toolbar />
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
					{/* The same transformer list the file is read and written
					    with, so what converts as you type is exactly what
					    survives a save. */}
					<MarkdownShortcutPlugin
						transformers={MARKDOWN_TRANSFORMERS}
					/>
					<ListPlugin />
					{/* Keeps link nodes tidy as they are edited, as well as
					    answering for the toggle command. */}
					<LinkPlugin />
					<Links />
					{/* Moving the caret is not an edit, or every click would
					    mark the document unsaved. */}
					<OnChangePlugin
						ignoreSelectionChange
						onChange={(state) =>
							onChange(state.read(() => $toMarkdown()))
						}
					/>
					<Shortcuts />
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
