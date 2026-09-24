// The compare view: the history down the left, and the chosen version beside
// the text as it is now, with what differs washed in. For the whole project,
// what changed is listed above, and an edited document is picked from it.

import { invoke } from "@tauri-apps/api/core";
import {
	type CSSProperties,
	type PointerEvent,
	type ReactNode,
	type RefObject,
	type UIEvent,
	useEffect,
	useLayoutEffect,
	useMemo,
	useRef,
	useState,
} from "react";
import { differences } from "./differences";
import { failure } from "./errors";
import Icon from "./Icon";
import {
	type Block,
	blocks,
	change,
	follow,
	type Run,
	tally,
} from "./lib/compare";
import { byDay, counts, detail, summary } from "./lib/versions";
import { timeOfDay } from "./dates";
import type { Changes, ProjectDocument, Version, VersionsScope } from "./types";
import { prose, words } from "./words";

type Props = {
	root: string;
	/** The open document, which This document compares. */
	document: ProjectDocument | null;
	scope: VersionsScope;
	/** The version the writer asked to compare, which starts as A. */
	version: string;
	fontSize: number;
	lineHeight: number;
	onScope: (scope: VersionsScope) => void;
	onBack: () => void;
};

// A document's text at A and on disk, which is B. Null `then` is a version
// from before the document existed.
type Texts = { then: string | null; now: string };

type Field = { index: number; text: string };

// A difference's mark on the strip, at a fraction of the way down B.
type Tick = { index: number; at: number; change: ReturnType<typeof change> };

type Pair = [number, number];

export default function Compare({
	root,
	document,
	scope,
	version,
	fontSize,
	lineHeight,
	onScope,
	onBack,
}: Props) {
	const open = scope === "document" ? document : null;
	const [versions, setVersions] = useState<Version[]>([]);
	const [chosen, setChosen] = useState(version);
	const [changes, setChanges] = useState<Changes | null>(null);
	// The edited document shown in the columns for the whole project.
	const [picked, setPicked] = useState<string | null>(null);
	const shown = open === null ? picked : open.id;
	const [texts, setTexts] = useState<Texts | null>(null);
	const [current, setCurrent] = useState<number | null>(null);
	const [error, setError] = useState<string | null>(null);
	const [ticks, setTicks] = useState<Tick[]>([]);
	const left = useRef<HTMLDivElement>(null);
	const right = useRef<HTMLDivElement>(null);
	const view = useRef<HTMLDivElement>(null);
	// Heights down A and B that line up, and the column whose scroll was set
	// by the other, so its own scroll event is not passed back.
	const pairs = useRef<Pair[]>([]);
	const driven = useRef<HTMLElement | null>(null);

	useEffect(() => {
		let live = true;
		invoke<Version[]>("list_versions", {
			root,
			document: open?.id ?? null,
		})
			.then((listed) => {
				if (live) {
					setVersions(listed);
				}
			})
			.catch((reason: unknown) => {
				if (live) {
					setError(failure(reason).message);
				}
			});
		return () => {
			live = false;
		};
	}, [root, open?.id]);

	const whole = open === null;
	useEffect(() => {
		if (!whole) {
			return;
		}
		let live = true;
		invoke<Changes>("version_changes", { root, a: chosen, b: null })
			.then((found) => {
				if (live) {
					setChanges(found);
					const edited = found.nodes
						.filter((change) => change.edited)
						.map((change) => change.id);
					setPicked((was) =>
						was !== null && edited.includes(was)
							? was
							: (edited[0] ?? null),
					);
				}
			})
			.catch((reason: unknown) => {
				if (live) {
					setError(failure(reason).message);
				}
			});
		return () => {
			live = false;
		};
	}, [root, whole, chosen]);

	useEffect(() => {
		if (shown === null) {
			setTexts(null);
			return;
		}
		let live = true;
		Promise.all([
			invoke<string | null>("version_text", {
				root,
				version: chosen,
				id: shown,
			}),
			invoke<string>("read_document", { root, id: shown }),
		])
			.then(([then, now]) => {
				if (live) {
					setTexts({ then, now });
					setCurrent(null);
					setError(null);
				}
			})
			.catch((reason: unknown) => {
				if (live) {
					setError(failure(reason).message);
				}
			});
		return () => {
			live = false;
		};
	}, [root, shown, chosen]);

	const before = texts?.then ?? "";
	const now = texts?.now ?? "";
	const found = useMemo(() => differences(before, now), [before, now]);
	const sides = useMemo(
		() => ({
			a: blocks(
				before,
				found.map((difference) => difference.then),
			),
			b: blocks(
				now,
				found.map((difference) => difference.now),
			),
		}),
		[before, now, found],
	);
	const { added, removed } = tally(before, now, found);

	// Changed fields have no place in the prose, so they sit above it as the
	// lines of front matter they are.
	function fieldsOf(text: string, side: "then" | "now"): Field[] {
		return found.flatMap((difference, index) =>
			difference.kind === "field"
				? [
						{
							index,
							text: text.slice(
								difference[side].from,
								difference[side].to,
							),
						},
					]
				: [],
		);
	}

	useLayoutEffect(() => {
		const a = left.current;
		const b = right.current;
		if (a === null || b === null) {
			return;
		}

		const measure = () => {
			const inA = marks(a);
			const inB = marks(b);
			const lined: Pair[] = [];
			const marked: Tick[] = [];
			found.forEach((difference, index) => {
				const there = inA.get(index);
				const here = inB.get(index);
				if (there !== undefined && here !== undefined) {
					lined.push(
						[there.top, here.top],
						[there.bottom, here.bottom],
					);
				}
				if (here !== undefined) {
					marked.push({
						index,
						at: here.top / b.scrollHeight,
						change: change(difference),
					});
				}
			});
			lined.push([a.scrollHeight, b.scrollHeight]);
			pairs.current = lined;
			setTicks(marked);
			place(b);
		};

		measure();
		// Wrapping changes with the window's width, and every height with it.
		const observer = new ResizeObserver(measure);
		observer.observe(a);
		observer.observe(b);
		return () => observer.disconnect();
	}, [found, sides]);

	// Where the strip shows B's visible part.
	function place(b: HTMLElement) {
		const style = view.current?.style;
		if (style !== undefined) {
			style.top = `${(b.scrollTop / b.scrollHeight) * 100}%`;
			style.height = `${(b.clientHeight / b.scrollHeight) * 100}%`;
		}
	}

	// Keeps the line across the middle of both columns on the same passage.
	function scrolled(
		source: HTMLElement,
		target: HTMLElement | null,
		lined: Pair[],
	) {
		if (driven.current === source) {
			driven.current = null;
			return;
		}
		if (target === null) {
			return;
		}
		const middle =
			follow(lined, source.scrollTop + source.clientHeight / 2) -
			target.clientHeight / 2;
		const top = Math.max(
			0,
			Math.min(middle, target.scrollHeight - target.clientHeight),
		);
		// A column already where it should be fires no scroll event to clear
		// `driven`, so it is only set when the column will move.
		if (Math.abs(top - target.scrollTop) >= 1) {
			driven.current = target;
			target.scrollTop = top;
		}
	}

	// Centres B on the height pressed on the strip, and A follows.
	function seek(event: PointerEvent<HTMLDivElement>) {
		const b = right.current;
		if (b === null) {
			return;
		}
		const box = event.currentTarget.getBoundingClientRect();
		b.scrollTop =
			((event.clientY - box.top) / box.height) * b.scrollHeight -
			b.clientHeight / 2;
	}

	// B leads when it has a mark for the difference, and A follows it.
	function go(index: number) {
		setCurrent(index);
		const selector = `[data-difference="${index}"]`;
		(
			right.current?.querySelector(selector) ??
			left.current?.querySelector(selector)
		)?.scrollIntoView({ block: "center" });
	}

	const today = new Date();
	const days = byDay(versions, today);
	if (days[0]?.heading !== "Today") {
		days.unshift({ heading: "Today", versions: [] });
	}
	const a = versions.find((listed) => listed.id === chosen);

	return (
		<div className="compare">
			<div className="compare__bar">
				<button
					type="button"
					className="compare__back"
					onClick={onBack}
				>
					<Icon name="chevron-left" />
					Back to writing
				</button>
			</div>

			<div className="compare__body">
				<nav className="compare__history" aria-label="Versions">
					<h2 className="compare__title">
						{open === null
							? "Versions of the whole project"
							: `Versions of ${open.title}`}
					</h2>
					{open !== null && (
						<p className="compare__trail">
							{open.trail.join(" · ")}
						</p>
					)}
					<div
						className="versions__scope compare__scope"
						role="group"
						aria-label="Versions of"
					>
						<button
							type="button"
							className="versions__scope-button"
							aria-pressed={open !== null}
							disabled={document === null}
							onClick={() => onScope("document")}
						>
							This document
						</button>
						<button
							type="button"
							className="versions__scope-button"
							aria-pressed={open === null}
							onClick={() => onScope("project")}
						>
							Whole project
						</button>
					</div>

					{days.map((day, index) => (
						<section key={day.heading} className="versions__day">
							<h3 className="versions__date">{day.heading}</h3>
							<ul className="versions__list">
								{index === 0 && (
									<li className="versions__item">
										<div className="versions__pick compare__now">
											<span
												className="versions__mark"
												aria-hidden="true"
											/>
											<span className="versions__name">
												Now (unsaved)
											</span>
											<span className="compare__letter">
												B
											</span>
										</div>
									</li>
								)}
								{day.versions.map((listed) => (
									<Row
										key={listed.id}
										version={listed}
										chosen={listed.id === chosen}
										onChoose={() => setChosen(listed.id)}
									/>
								))}
							</ul>
						</section>
					))}
				</nav>

				<div className="compare__main">
					{whole && changes !== null && (
						<ul
							className="compare__changes"
							aria-label="What changed"
						>
							{changes.book && (
								<Change what="Book details changed" />
							)}
							{changes.cover && <Change what="Cover changed" />}
							{changes.nodes.map((change) => (
								<Change
									key={change.id}
									{...summary(change)}
									picked={
										change.edited
											? change.id === picked
											: null
									}
									onPick={() => setPicked(change.id)}
								/>
							))}
							{changes.nodes.length === 0 &&
								!changes.book &&
								!changes.cover && (
									<Change what="Nothing has changed since this version" />
								)}
						</ul>
					)}
					<div
						className="compare__columns"
						style={
							{
								"--font-size": `${fontSize}px`,
								"--line-height": `${lineHeight}`,
							} as CSSProperties
						}
					>
						<Column
							letter="A"
							title={a === undefined ? "" : (a.name ?? detail(a))}
							detail={
								a === undefined
									? ""
									: `${byDay([a], today)[0].heading} ${timeOfDay(a.at)} · ${words(prose(before)).toLocaleString()} words`
							}
							side="then"
							fields={fieldsOf(before, "then")}
							body={sides.a}
							current={current}
							missing={texts !== null && texts.then === null}
							scroller={left}
							onScroll={(event) =>
								scrolled(
									event.currentTarget,
									right.current,
									pairs.current,
								)
							}
						/>
						<Column
							letter="B"
							title="Now"
							detail={[
								"unsaved",
								`${words(prose(now)).toLocaleString()} words`,
								counts(added, removed),
							]
								.filter((part) => part !== "")
								.join(" · ")}
							side="now"
							fields={fieldsOf(now, "now")}
							body={sides.b}
							current={current}
							missing={false}
							scroller={right}
							onScroll={(event) => {
								place(event.currentTarget);
								scrolled(
									event.currentTarget,
									left.current,
									pairs.current.map(([a, b]): Pair => [b, a]),
								);
							}}
						>
							{/* Out of the reading order: first and next do the
							    same for a keyboard. */}
							<div
								className="compare__map"
								aria-hidden="true"
								onPointerDown={(event) => {
									const tick = (event.target as HTMLElement)
										.dataset.tick;
									if (tick !== undefined) {
										go(Number(tick));
										return;
									}
									// Held, so dragging keeps scrolling even off
									// the strip.
									event.currentTarget.setPointerCapture(
										event.pointerId,
									);
									seek(event);
								}}
								onPointerMove={(event) => {
									if (
										event.currentTarget.hasPointerCapture(
											event.pointerId,
										)
									) {
										seek(event);
									}
								}}
							>
								<div ref={view} className="compare__view" />
								{ticks.map((tick) => (
									<span
										key={tick.index}
										className={`compare__tick compare__tick--${tick.change}`}
										style={{ top: `${tick.at * 100}%` }}
										data-tick={tick.index}
									/>
								))}
							</div>
						</Column>
					</div>

					<div className="compare__foot">
						<span className="compare__count">
							{found.length === 1
								? "1 difference"
								: `${found.length.toLocaleString()} differences`}
						</span>
						<span aria-hidden="true">·</span>
						<button
							type="button"
							className="compare__step"
							disabled={found.length === 0}
							onClick={() => go(0)}
						>
							first
						</button>
						<span aria-hidden="true">·</span>
						<button
							type="button"
							className="compare__step"
							disabled={found.length === 0}
							onClick={() =>
								go(
									current === null
										? 0
										: (current + 1) % found.length,
								)
							}
						>
							next
						</button>
						{/* Laid over the foot, so a message does not move anything. */}
						<p className="compare__trouble" role="alert">
							{error ?? ""}
						</p>
					</div>
				</div>
			</div>
		</div>
	);
}

function Row({
	version,
	chosen,
	onChoose,
}: {
	version: Version;
	chosen: boolean;
	onChoose: () => void;
}) {
	const named = version.name !== null;
	return (
		<li
			className={[
				"versions__item",
				named ? "versions__item--named" : "",
				chosen ? "versions__item--selected" : "",
			]
				.filter((part) => part !== "")
				.join(" ")}
		>
			<button
				type="button"
				className="versions__pick"
				aria-pressed={chosen}
				onClick={onChoose}
			>
				<span className="versions__mark" aria-hidden="true" />
				<span className="versions__text">
					{named && (
						<span className="versions__name">{version.name}</span>
					)}
					<span className="versions__detail">{detail(version)}</span>
				</span>
				{chosen && <span className="compare__letter">A</span>}
			</button>
		</li>
	);
}

// A row of what changed. Only an edited document has words to compare, so
// only it can be picked; `picked` is null for the rest.
function Change({
	name,
	what,
	picked = null,
	onPick,
}: {
	name?: string;
	what: string;
	picked?: boolean | null;
	onPick?: () => void;
}) {
	const body = (
		<>
			{name !== undefined && (
				<span className="compare__change-name">{name}</span>
			)}
			<span className="compare__what">{what}</span>
		</>
	);
	return (
		<li className="compare__change">
			{picked === null ? (
				<div className="compare__pick">{body}</div>
			) : (
				<button
					type="button"
					className="compare__pick"
					aria-pressed={picked}
					onClick={onPick}
				>
					{body}
				</button>
			)}
		</li>
	);
}

function Column({
	letter,
	title,
	detail,
	side,
	fields,
	body,
	current,
	missing,
	scroller,
	onScroll,
	children,
}: {
	letter: string;
	title: string;
	detail: string;
	/** Then is washed as taken out, now as put in. */
	side: "then" | "now";
	fields: Field[];
	body: Block[];
	current: number | null;
	missing: boolean;
	scroller: RefObject<HTMLDivElement | null>;
	onScroll: (event: UIEvent<HTMLDivElement>) => void;
	children?: ReactNode;
}) {
	return (
		<article className={`compare__column compare__column--${side}`}>
			<header className="compare__head">
				<span className="compare__letter">{letter}</span>
				<span className="compare__heading">
					<span className="compare__name">{title}</span>
					<span className="compare__detail">{detail}</span>
				</span>
			</header>
			<div className="compare__pane">
				<div
					className="compare__scroll"
					ref={scroller}
					onScroll={onScroll}
				>
					{missing && (
						<p className="compare__missing">
							This document did not exist yet in this version.
						</p>
					)}
					{fields.map((field) => (
						<p key={field.index} className="compare__field">
							<Wash difference={field.index} current={current}>
								{field.text}
							</Wash>
						</p>
					))}
					<div className="editor__text compare__text">
						{body.map((block, index) =>
							draw(block, index, current),
						)}
					</div>
				</div>
				{children}
			</div>
		</article>
	);
}

// Where each difference's marks sit down a column, from the top of its text.
function marks(
	scroller: HTMLElement,
): Map<number, { top: number; bottom: number }> {
	const origin = scroller.getBoundingClientRect().top - scroller.scrollTop;
	const found = new Map<number, { top: number; bottom: number }>();
	scroller
		.querySelectorAll<HTMLElement>("[data-difference]")
		.forEach((mark) => {
			const index = Number(mark.dataset.difference);
			const box = mark.getBoundingClientRect();
			const seen = found.get(index);
			found.set(index, {
				top: Math.min(seen?.top ?? Infinity, box.top - origin),
				bottom: Math.max(
					seen?.bottom ?? -Infinity,
					box.bottom - origin,
				),
			});
		});
	return found;
}

function draw(block: Block, key: number, current: number | null): ReactNode {
	switch (block.kind) {
		case "break":
			return <hr key={key} className="editor__rule" />;
		case "heading": {
			const Heading = `h${block.level}` as "h1";
			return (
				<Heading
					key={key}
					className={`editor__heading editor__heading--${block.level}`}
				>
					{block.runs.map((run, index) =>
						drawRun(run, index, current),
					)}
				</Heading>
			);
		}
		case "quote":
			return (
				<blockquote key={key} className="editor__quote">
					{block.runs.map((run, index) =>
						drawRun(run, index, current),
					)}
				</blockquote>
			);
		case "item":
		case "paragraph":
			return (
				<p
					key={key}
					className={
						block.kind === "item"
							? "editor__paragraph compare__item"
							: "editor__paragraph"
					}
				>
					{block.runs.map((run, index) =>
						drawRun(run, index, current),
					)}
				</p>
			);
	}
}

function drawRun(run: Run, key: number, current: number | null): ReactNode {
	// An empty run stays empty, so its mark draws as a caret.
	let drawn: ReactNode = run.text === "" ? null : run.text;
	if (run.em && drawn !== null) {
		drawn = <em>{drawn}</em>;
	}
	if (run.strong && drawn !== null) {
		drawn = <strong>{drawn}</strong>;
	}
	return run.difference === null ? (
		<span key={key}>{drawn}</span>
	) : (
		<Wash key={key} difference={run.difference} current={current}>
			{drawn}
		</Wash>
	);
}

function Wash({
	difference,
	current,
	children,
}: {
	difference: number;
	current: number | null;
	children: ReactNode;
}) {
	return (
		<mark
			className={
				difference === current
					? "compare__wash compare__wash--current"
					: "compare__wash"
			}
			data-difference={difference}
		>
			{children}
		</mark>
	);
}
