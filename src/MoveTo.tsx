// The second face of a row's ⋮ menu: every folder in the project, indented,
// with the ones that cannot take this node dimmed. It reads the tree itself
// rather than being handed one, so the sidebar and an overview both get it
// without either having to carry a copy of the project around.

import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import Icon from "./Icon";
import { MenuItem } from "./Menu";
import type { TreeNode } from "./types";
import type { Moving } from "./tree";
import { destinations } from "./tree";
import { failure } from "./errors";

type Props = {
	root: string;
	moving: Moving;
	onBack: () => void;
	/** The folder to move into, and the index that puts the node last in it. */
	onChoose: (parentId: string, index: number) => void;
};

type Tree =
	| { kind: "reading" }
	| { kind: "read"; nodes: TreeNode[] }
	| { kind: "error"; message: string };

export default function MoveTo({ root, moving, onBack, onChoose }: Props) {
	const [tree, setTree] = useState<Tree>({ kind: "reading" });
	const back = useRef<HTMLButtonElement>(null);

	// The item that opened this list has just been unmounted, taking the
	// keyboard with it if nothing here claims it.
	useEffect(() => {
		back.current?.focus();
	}, []);

	useEffect(() => {
		let open = true;
		invoke<TreeNode[]>("document_tree", { root })
			.then((nodes) => {
				if (open) {
					setTree({ kind: "read", nodes });
				}
			})
			.catch((error: unknown) => {
				if (open) {
					setTree({ kind: "error", message: failure(error).message });
				}
			});
		// The menu is dismissed by clicking away, which can happen while the
		// read is still out.
		return () => {
			open = false;
		};
	}, [root]);

	return (
		<>
			<button
				ref={back}
				type="button"
				className="menu__item menu__back"
				onClick={onBack}
			>
				<Icon name="chevron-left" />
				Move to
			</button>

			{tree.kind === "reading" && <p className="menu__note">Reading…</p>}
			{tree.kind === "error" && (
				<p className="menu__note" role="alert">
					{tree.message}
				</p>
			)}
			{tree.kind === "read" &&
				destinations(tree.nodes, moving).map((where) => (
					<MenuItem
						key={where.id}
						depth={where.depth}
						disabled={!where.allowed}
						onSelect={() => onChoose(where.id, where.children)}
					>
						{where.name}
					</MenuItem>
				))}
		</>
	);
}
