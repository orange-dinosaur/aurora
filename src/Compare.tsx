// The compare view: one document's history down the left, and the chosen
// version beside the text as it is now, with what differs washed in.

import { invoke } from "@tauri-apps/api/core";
import {
	type CSSProperties,
	type ReactNode,
	useEffect,
	useMemo,
	useRef,
	useState,
} from "react";
import { differences } from "./differences";
import { failure } from "./errors";
import Icon from "./Icon";
import { type Block, blocks, type Run, tally } from "./lib/compare";
import { byDay, counts, detail } from "./lib/versions";
import { timeOfDay } from "./dates";
import type { ProjectDocument, Version } from "./types";
import { prose, words } from "./words";

type Props = {
	root: string;
	document: ProjectDocument;
	/** The version the writer asked to compare, which starts as A. */
	version: string;
	/** The document's text on screen, which is B. */
	now: string;
	fontSize: number;
	lineHeight: number;
	onBack: () => void;
};

// A version's text once read. Null text is a version from before the
// document existed.
type Then = { text: string | null };

type Field = { index: number; text: string };

export default function Compare({
	root,
	document,
	version,
	now,
	fontSize,
	lineHeight,
	onBack,
}: Props) {
	const [versions, setVersions] = useState<Version[]>([]);
	const [chosen, setChosen] = useState(version);
	const [then, setThen] = useState<Then | null>(null);
	const [current, setCurrent] = useState<number | null>(null);
	const [error, setError] = useState<string | null>(null);
	const columns = useRef<HTMLDivElement>(null);

	useEffect(() => {
		let live = true;
		invoke<Version[]>("list_versions", { root, document: document.id })
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
	}, [root, document.id]);

	useEffect(() => {
		let live = true;
		invoke<string | null>("version_text", {
			root,
			version: chosen,
			id: document.id,
		})
			.then((text) => {
				if (live) {
					setThen({ text });
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
	}, [root, document.id, chosen]);

	const before = then?.text ?? "";
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

	useEffect(() => {
		if (current === null) {
			return;
		}
		columns.current
			?.querySelectorAll(`[data-difference="${current}"]`)
			.forEach((mark) => mark.scrollIntoView({ block: "center" }));
	}, [current]);

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
						Versions of {document.title}
					</h2>
					<p className="compare__trail">
						{document.trail.join(" · ")}
					</p>

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
					<div
						className="compare__columns"
						ref={columns}
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
							fields={fieldsOf(before, "then")}
							body={sides.a}
							current={current}
							missing={then !== null && then.text === null}
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
							fields={fieldsOf(now, "now")}
							body={sides.b}
							current={current}
							missing={false}
						/>
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
							onClick={() => setCurrent(0)}
						>
							first
						</button>
						<span aria-hidden="true">·</span>
						<button
							type="button"
							className="compare__step"
							disabled={found.length === 0}
							onClick={() =>
								setCurrent((at) =>
									at === null ? 0 : (at + 1) % found.length,
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

function Column({
	letter,
	title,
	detail,
	fields,
	body,
	current,
	missing,
}: {
	letter: string;
	title: string;
	detail: string;
	fields: Field[];
	body: Block[];
	current: number | null;
	missing: boolean;
}) {
	return (
		<article className="compare__column">
			<header className="compare__head">
				<span className="compare__letter">{letter}</span>
				<span className="compare__heading">
					<span className="compare__name">{title}</span>
					<span className="compare__detail">{detail}</span>
				</span>
			</header>
			<div className="compare__scroll">
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
					{body.map((block, index) => draw(block, index, current))}
				</div>
			</div>
		</article>
	);
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
