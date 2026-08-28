import { LexicalComposer } from "@lexical/react/LexicalComposer";
import { ContentEditable } from "@lexical/react/LexicalContentEditable";
import { LexicalErrorBoundary } from "@lexical/react/LexicalErrorBoundary";
import { HistoryPlugin } from "@lexical/react/LexicalHistoryPlugin";
import { LinkPlugin } from "@lexical/react/LexicalLinkPlugin";
import { ListPlugin } from "@lexical/react/LexicalListPlugin";
import { MarkdownShortcutPlugin } from "@lexical/react/LexicalMarkdownShortcutPlugin";
import { OnChangePlugin } from "@lexical/react/LexicalOnChangePlugin";
import { RichTextPlugin } from "@lexical/react/LexicalRichTextPlugin";
import type { EditorState, EditorThemeClasses } from "lexical";
import type { Preferences } from "./types";
import { useCallback, useState, type CSSProperties } from "react";
import {
	$fromMarkdown,
	$toMarkdown,
	EDITOR_NODES,
	MARKDOWN_TRANSFORMERS,
} from "./markdown";
import { shortcutLabel, TOOLBAR } from "./formatting";
import Focus from "./Focus";
import Links from "./Links";
import Shortcuts from "./Shortcuts";
import SlashMenu from "./SlashMenu";
import Toolbar from "./Toolbar";
import Typewriter from "./Typewriter";
import Typography from "./Typography";
import { characters, charactersWithoutSpaces, words } from "./words";

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

function counted(text: string) {
	return {
		words: words(text),
		characters: characters(text),
		tight: charactersWithoutSpaces(text),
	};
}

/** The status line's tally, in the order a writer scans it. */
function tallied(counts: ReturnType<typeof counted>): string {
	return [
		counts.words === 1
			? "1 word"
			: `${counts.words.toLocaleString()} words`,
		counts.characters === 1
			? "1 character"
			: `${counts.characters.toLocaleString()} characters`,
		`${counts.tight.toLocaleString()} without spaces`,
	].join(" · ");
}

type Props = {
	title: string;
	text: string;
	dirty: boolean;
	saving: boolean;
	missing: boolean;
	error: string | null;
	preferences: Preferences;
	onChange: (text: string) => void;
	onRestore: () => void;
	onPreferences: (next: Preferences) => void;
};

export default function Editor({
	title,
	text,
	dirty,
	saving,
	missing,
	error,
	preferences,
	onChange,
	onRestore,
	onPreferences,
}: Props) {
	const toggle = useCallback(
		() => onPreferences({ ...preferences, toolbar: !preferences.toolbar }),
		[preferences, onPreferences],
	);
	const note = error ?? (saving ? "Saving…" : dirty ? "Unsaved" : "Saved");
	// Counted from the markdown the editor would save, which is the same text
	// the Rust side counts when it summarises the file. The three numbers are
	// taken from one draft together, so they cannot describe different text.
	const [tally, setTally] = useState(() => counted(text));
	const edited = useCallback(
		(state: EditorState) => {
			const markdown = state.read(() => $toMarkdown());
			setTally(counted(markdown));
			onChange(markdown);
		},
		[onChange],
	);
	const look = ["editor"];
	if (preferences.focus) {
		look.push("editor--focus");
	}
	if (preferences.typewriter) {
		look.push("editor--typewriter");
	}

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
		<div
			className={look.join(" ")}
			// The page's own measurements, handed to the stylesheet. `ch` is
			// read against this element's font size, which is why the size is
			// set here and not further down.
			style={
				{
					"--measure": `${preferences.measure}ch`,
					"--font-size": `${preferences.fontSize}px`,
					"--line-height": `${preferences.lineHeight}`,
				} as CSSProperties
			}
		>
			<div className="editor__head">
				<h2 className="editor__title">{title}</h2>
				<div className="editor__tools">
					<Typography
						preferences={preferences}
						onPreferences={onPreferences}
					/>
					{/* Turned on and off while writing rather than set once,
					    so it stays in reach instead of going in the panel
					    above. */}
					<button
						type="button"
						className="editor__toggle"
						aria-pressed={preferences.focus}
						aria-label="Focus mode"
						title="Dim other paragraphs"
						onClick={() =>
							onPreferences({
								...preferences,
								focus: !preferences.focus,
							})
						}
					>
						{"\u25d0"}
					</button>
					<button
						type="button"
						className="editor__toggle"
						aria-pressed={preferences.typewriter}
						aria-label="Typewriter scrolling"
						title="Hold the line being written on"
						onClick={() =>
							onPreferences({
								...preferences,
								typewriter: !preferences.typewriter,
							})
						}
					>
						{"\u2195"}
					</button>
					{/* The one way back once the bar is gone, so it stays on
					    screen whichever way round it is. */}
					<button
						type="button"
						className="editor__toggle"
						aria-expanded={preferences.toolbar}
						aria-label="Formatting bar"
						title={`Formatting bar (${shortcutLabel(TOOLBAR)})`}
						aria-keyshortcuts={shortcutLabel(TOOLBAR)}
						onClick={toggle}
					>
						{preferences.toolbar ? "\u2304" : "\u203a"}
					</button>
				</div>
			</div>
			<LexicalComposer initialConfig={config}>
				{preferences.toolbar && <Toolbar />}
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
					<OnChangePlugin ignoreSelectionChange onChange={edited} />
					<Shortcuts onToolbar={toggle} />
					<Focus on={preferences.focus} />
					<Typewriter on={preferences.typewriter} />
					<SlashMenu />
				</div>
			</LexicalComposer>
			<div className="editor__foot">
				<p className="editor__count">{tallied(tally)}</p>
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
		</div>
	);
}
