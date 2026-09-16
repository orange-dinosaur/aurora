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
import Icon from "./Icon";
import type { Preferences } from "./types";
import {
	useCallback,
	useEffect,
	useRef,
	useState,
	type CSSProperties,
} from "react";
import {
	$fromMarkdown,
	$toMarkdown,
	EDITOR_NODES,
	MARKDOWN_TRANSFORMERS,
} from "./markdown";
import { useFields, type FieldsHandle } from "./fields";
import type { Seed } from "./lib/find";
import { FIND, OUTLINE, shortcutLabel, TOOLBAR } from "./formatting";
import Find from "./Find";
import Focus from "./Focus";
import Links from "./Links";
import { useOutline, type OutlineHandle } from "./lib/outline";
import Shortcuts from "./Shortcuts";
import SlashMenu from "./SlashMenu";
import Target from "./Target";
import Toolbar from "./Toolbar";
import Typewriter from "./Typewriter";
import Typography from "./Typography";
import { characters, charactersWithoutSpaces, prose, words } from "./words";

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

/**
 * Hands the open document's fields to whatever is showing them, since the panel
 * that does sits beside the editor rather than inside it and cannot reach the
 * composer's context from there. Only the editor whose tab is showing reports:
 * every open document stays mounted, and they would otherwise all answer.
 */
function Fields({
	onFields,
}: {
	onFields: (fields: FieldsHandle | null) => void;
}) {
	const fields = useFields();

	useEffect(() => onFields(fields), [fields, onFields]);
	useEffect(() => () => onFields(null), [onFields]);

	return null;
}

/** The headings, sent the same way and for the same reason as the fields. */
function Outline({
	onOutline,
}: {
	onOutline: (outline: OutlineHandle | null) => void;
}) {
	const outline = useOutline();

	useEffect(() => onOutline(outline), [outline, onOutline]);
	useEffect(() => () => onOutline(null), [onOutline]);

	return null;
}

function counted(text: string) {
	const body = prose(text);
	return {
		words: words(body),
		characters: characters(body),
		tight: charactersWithoutSpaces(body),
	};
}

/** The rest of the status line. The words are `Target`'s to report. */
function tallied(counts: ReturnType<typeof counted>): string {
	const characters =
		counts.characters === 1
			? "1 character"
			: `${counts.characters.toLocaleString()} characters`;
	return ` · ${characters} · ${counts.tight.toLocaleString()} without spaces`;
}

type Props = {
	title: string;
	/** The folders it sits in, from its section down, drawn above the title. */
	trail: string[];
	text: string;
	/** Whether this is the tab being looked at, and so the one that reports. */
	active: boolean;
	dirty: boolean;
	saving: boolean;
	missing: boolean;
	error: string | null;
	target: number | null;
	/** A hit clicked in search, which opens Find on that word. */
	seed: Seed | null;
	preferences: Preferences;
	onChange: (text: string) => void;
	/** How many words a change added, or took away when it is negative. */
	onWrote: (delta: number) => void;
	onFields: (fields: FieldsHandle | null) => void;
	onOutline: (outline: OutlineHandle | null) => void;
	onRestore: () => void;
	onPreferences: (next: Preferences) => void;
	onTarget: (target: number | null) => void;
};

export default function Editor({
	title,
	trail,
	text,
	active,
	dirty,
	saving,
	missing,
	error,
	target,
	seed,
	preferences,
	onChange,
	onWrote,
	onFields,
	onOutline,
	onRestore,
	onPreferences,
	onTarget,
}: Props) {
	const toggle = useCallback(
		() => onPreferences({ ...preferences, toolbar: !preferences.toolbar }),
		[preferences, onPreferences],
	);
	// The headings live in the right sidebar now, so the button and its
	// shortcut bring that panel round to them instead of opening a column.
	const showing =
		preferences.rightSidebar && preferences.rightSidebarTab === "synopsis";
	const outline = useCallback(
		() =>
			onPreferences({
				...preferences,
				rightSidebar: !showing,
				rightSidebarTab: "synopsis",
			}),
		[preferences, showing, onPreferences],
	);
	const note = error ?? (saving ? "Saving…" : dirty ? "Unsaved" : "Saved");
	// Counted from the markdown the editor would save, less the front matter,
	// which is exactly what the Rust side counts when it summarises the file.
	// The three numbers are taken from one draft together, so they cannot
	// describe different text.
	const [tally, setTally] = useState(() => counted(text));
	// What the last change left behind, so the next one can report the words it
	// moved rather than the words the document holds. Seeding it from the
	// document is what keeps opening a tab from counting as writing.
	const was = useRef(tally.words);
	const [finding, setFinding] = useState(false);
	// Bumped rather than just set, so asking for find while the panel is
	// already open takes the caret back to the field instead of doing nothing.
	const [asked, setAsked] = useState(0);
	const [replacing, setReplacing] = useState(false);
	const find = useCallback(() => {
		setFinding(true);
		setAsked((times) => times + 1);
	}, []);
	// The same panel, opened at the other end of the job.
	const replace = useCallback(() => {
		setFinding(true);
		setReplacing(true);
		setAsked((times) => times + 1);
	}, []);
	// A hit clicked in search opens the panel on that word. The seed is a new
	// object per click, so this answers a second click on the same hit as well
	// as the first, and it fires on mount for a document that search opened.
	useEffect(() => {
		if (seed === null) {
			return;
		}

		setFinding(true);
		setAsked((times) => times + 1);
	}, [seed]);

	const edited = useCallback(
		(state: EditorState) => {
			const markdown = state.read(() => $toMarkdown());
			const next = counted(markdown);
			setTally(next);
			// Moving the caret is a change like any other here, and it has
			// nothing to report.
			if (next.words !== was.current) {
				onWrote(next.words - was.current);
				was.current = next.words;
			}
			onChange(markdown);
		},
		[onChange, onWrote],
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
				<div className="editor__heading">
					{/* Where the document sits, above its name. Always in the
					    tree, so the title never moves as the writer opens
					    something filed one level deeper. */}
					<p className="editor__where">{trail.join(" · ")}</p>
					<h2 className="editor__title">{title}</h2>
				</div>
				<div className="editor__tools">
					<Typography
						preferences={preferences}
						onPreferences={onPreferences}
					/>
					{/* The only way to the panel that does not need the caret
					    to be in the text: Ctrl+F is the editor's own key, so
					    it cannot answer when the focus is elsewhere. */}
					<button
						type="button"
						className="editor__toggle"
						aria-expanded={finding}
						aria-label="Find in this document"
						title={`Find (${shortcutLabel(FIND)})`}
						aria-keyshortcuts={shortcutLabel(FIND)}
						onClick={find}
					>
						<Icon name="search" />
					</button>
					<button
						type="button"
						className="editor__toggle"
						aria-pressed={showing}
						aria-label="Outline"
						title={`Outline (${shortcutLabel(OUTLINE)})`}
						aria-keyshortcuts={shortcutLabel(OUTLINE)}
						onClick={outline}
					>
						<Icon name="list" />
					</button>
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
						<Icon name="contrast" />
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
						<Icon name="arrows-vertical" />
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
						<Icon
							name={
								preferences.toolbar
									? "chevron-down"
									: "chevron-right"
							}
						/>
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
					{active && <Fields onFields={onFields} />}
					{active && <Outline onOutline={onOutline} />}
					<Shortcuts
						onToolbar={toggle}
						onOutline={outline}
						onFind={find}
						onReplace={replace}
					/>
					{finding && (
						<Find
							asked={asked}
							seed={seed}
							replacing={replacing}
							onReplacing={setReplacing}
							onClose={() => {
								setFinding(false);
								setReplacing(false);
							}}
						/>
					)}
					<Focus on={preferences.focus} />
					<Typewriter on={preferences.typewriter} />
					<SlashMenu />
				</div>
			</LexicalComposer>
			<div className="editor__foot">
				<p className="editor__count">
					<Target
						words={tally.words}
						target={target}
						onTarget={onTarget}
					/>
					{tallied(tally)}
				</p>
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
			{/* The number above is the real report; this is only its shape,
			    and there is nothing to draw until a target is set. */}
			{target !== null && (
				<div
					className="editor__progress"
					style={
						{
							"--progress": `${Math.min(100, (tally.words / target) * 100)}%`,
						} as CSSProperties
					}
					role="progressbar"
					aria-label="Progress towards the word target"
					aria-valuenow={tally.words}
					aria-valuemin={0}
					aria-valuemax={target}
				/>
			)}
		</div>
	);
}
