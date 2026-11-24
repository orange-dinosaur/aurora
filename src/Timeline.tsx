// Every scene of the Manuscript, or of one story in a collection, laid out
// left to right in story order, once in each row whose character it names.
// It stands over the binder and the tabs, and Escape goes back to them.

import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Timeline as Laid } from "./bindings/Timeline";
import { useCorpus } from "./corpus";
import { failure } from "./errors";
import { useFormat } from "./projectFormat";
import { rowsOf } from "./timeline";
import type { FolderRef } from "./tree";

type Props = {
	root: string;
	folder: FolderRef;
	changed: number;
	live: Map<string, string>;
	onBack: () => void;
};

type Read =
	| { kind: "reading" }
	| { kind: "ready"; laid: Laid }
	| { kind: "failed"; message: string };

export default function Timeline({
	root,
	folder,
	changed,
	live,
	onBack,
}: Props) {
	const [read, setRead] = useState<Read>({ kind: "reading" });
	const screenplay = useFormat() === "screenplay";

	useEffect(() => {
		let gone = false;
		invoke<Laid>("timeline", { root, folder: folder.id })
			.then((laid) => {
				if (!gone) {
					setRead({ kind: "ready", laid });
				}
			})
			.catch((error: unknown) => {
				if (!gone) {
					setRead({
						kind: "failed",
						message: failure(error).message,
					});
				}
			});
		return () => {
			gone = true;
		};
	}, [root, folder.id, changed]);

	useEffect(() => {
		function escape(event: KeyboardEvent) {
			// A menu open over the timeline takes its own Escape first.
			if (event.key === "Escape" && !event.defaultPrevented) {
				onBack();
			}
		}
		document.addEventListener("keydown", escape);
		return () => document.removeEventListener("keydown", escape);
	}, [onBack]);

	const { corpus, documents } = useCorpus({
		root,
		moved: changed,
		live,
		wanted: true,
	});

	const scenes = read.kind === "ready" ? read.laid.scenes : [];
	const rows = useMemo(
		() =>
			rowsOf(
				scenes.map((scene) => scene.id),
				documents,
				"Characters",
				screenplay,
			),
		[scenes, documents, screenplay],
	);

	return (
		<section className="timeline" aria-label={`Timeline of ${folder.name}`}>
			<header className="timeline__head">
				<p className="timeline__of">Timeline</p>
				<h1 className="timeline__title">{folder.name}</h1>
			</header>
			{read.kind === "failed" ? (
				<p className="timeline__note" role="alert">
					{read.message}
				</p>
			) : corpus.kind === "failed" ? (
				<p className="timeline__note" role="alert">
					{corpus.message}
				</p>
			) : read.kind === "reading" || corpus.kind !== "ready" ? (
				<p className="timeline__note">Reading the scenes…</p>
			) : scenes.length === 0 ? (
				<p className="timeline__note">No scenes here yet.</p>
			) : (
				<div className="timeline__scroll">
					<div
						className="timeline__grid"
						style={{
							gridTemplateColumns: `var(--timeline-label) repeat(${scenes.length}, var(--timeline-block))`,
						}}
					>
						<span className="timeline__corner" />
						{scenes.map((scene, at) => (
							<span
								key={scene.id}
								className="timeline__tick"
								title={scene.title}
							>
								{at + 1}
							</span>
						))}
						{rows.map((row) => (
							<div
								key={row.subject?.id ?? ""}
								className="timeline__row"
							>
								<span className="timeline__label">
									{row.subject?.title ?? "No one"}
								</span>
								{scenes.map((scene) =>
									row.scenes.has(scene.id) ? (
										<span
											key={scene.id}
											className="timeline__block"
											title={scene.title}
										>
											{scene.title}
										</span>
									) : (
										<span key={scene.id} />
									),
								)}
							</div>
						))}
					</div>
				</div>
			)}
		</section>
	);
}
