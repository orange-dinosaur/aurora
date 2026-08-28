import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { failure } from "./errors";

type Props = {
	name: string;
	root: string;
	sidebar: boolean;
	onSidebar: (open: boolean) => void;
	// The manifest has been read again, so whatever is showing it should look
	// at it afresh.
	onRefreshed: () => void;
};

export default function Titlebar({
	name,
	root,
	sidebar,
	onSidebar,
	onRefreshed,
}: Props) {
	const [busy, setBusy] = useState(false);
	const [message, setMessage] = useState<string | null>(null);

	// `refresh_documents` looks at the folder again before rewriting the
	// manifest, for anything that changed outside Aurora.
	async function refresh() {
		setBusy(true);
		try {
			await invoke("refresh_documents", { root });
			setMessage(null);
			onRefreshed();
		} catch (error) {
			setMessage(failure(error).message);
		} finally {
			setBusy(false);
		}
	}

	return (
		<header className="titlebar">
			<h1 className="titlebar__name">{name}</h1>
			<span className="titlebar__path" title={root}>
				{root}
			</span>

			<button
				type="button"
				className="titlebar__button"
				aria-label={busy ? "Refreshing…" : "Refresh documents"}
				disabled={busy}
				onClick={() => void refresh()}
			>
				↻
			</button>
			<button
				type="button"
				className="titlebar__button"
				aria-label={sidebar ? "Hide sidebar" : "Show sidebar"}
				aria-pressed={sidebar}
				onClick={() => onSidebar(!sidebar)}
			>
				◧
			</button>

			{/* Out of flow, so a failed refresh cannot push the writing down. */}
			<p className="titlebar__message" role="status">
				{message}
			</p>
		</header>
	);
}
