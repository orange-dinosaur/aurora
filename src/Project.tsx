import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import Sidebar from "./Sidebar";
import type { ProjectDocument } from "./Sidebar";

type Props = {
	name: string;
	root: string;
	onClose: () => void;
};

type Reading =
	| { kind: "none" }
	| { kind: "loading"; document: ProjectDocument }
	| { kind: "ready"; document: ProjectDocument; text: string }
	| { kind: "error"; document: ProjectDocument; message: string };

export default function Project({ name, root, onClose }: Props) {
	const [reading, setReading] = useState<Reading>({ kind: "none" });
	const open = reading.kind === "none" ? null : reading.document;

	useEffect(() => {
		if (open === null) {
			return;
		}

		// Clicking through the sidebar quickly leaves earlier reads in flight;
		// only the newest may set the text.
		let current = true;
		invoke<string>("read_document", { root, id: open.id })
			.then((text) => {
				if (current) {
					setReading({ kind: "ready", document: open, text });
				}
			})
			.catch((error: unknown) => {
				if (current) {
					setReading({
						kind: "error",
						document: open,
						message: String(error),
					});
				}
			});

		return () => {
			current = false;
		};
	}, [root, open]);

	return (
		<section className="project">
			<header className="project__header">
				<div>
					<h1 className="project__title">{name}</h1>
					<p className="project__path">
						<code>{root}</code>
					</p>
				</div>
				<button
					type="button"
					className="project__close"
					onClick={onClose}
				>
					Close project
				</button>
			</header>

			<div className="project__body">
				<Sidebar
					root={root}
					selectedId={open?.id ?? null}
					onSelect={(document) =>
						setReading({ kind: "loading", document })
					}
				/>
				<div className="project__main">
					{reading.kind === "none" ? (
						<p className="project__empty">
							Choose a document to read.
						</p>
					) : (
						<article className="reader">
							<h2 className="reader__title">
								{reading.document.title}
							</h2>
							{reading.kind === "loading" && (
								<p className="reader__note">Opening…</p>
							)}
							{reading.kind === "error" && (
								<p className="reader__note reader__note--error">
									{reading.message}
								</p>
							)}
							{reading.kind === "ready" &&
								(reading.text === "" ? (
									<p className="reader__note">
										This document is empty.
									</p>
								) : (
									<pre className="reader__text">
										{reading.text}
									</pre>
								))}
						</article>
					)}
				</div>
			</div>
		</section>
	);
}
