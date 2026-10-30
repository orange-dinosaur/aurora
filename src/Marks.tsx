// The chosen take as a page that cannot be typed in, with what changed since
// another take marked on it. It stands in for the writing surface.

import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import { useEffect, useMemo } from "react";
import { draw } from "./Blocks";
import { marked } from "./lib/compare";

type Props = {
	/** The document's text as it stands, unsaved words and all. */
	live: string;
	/** The other take's prose. */
	prose: string;
	onClose: () => void;
};

export default function Marks({ live, prose, onClose }: Props) {
	const [editor] = useLexicalComposerContext();
	const { body, cut } = useMemo(() => marked(live, prose), [live, prose]);

	// After the surface is back, since a hidden one cannot take the caret.
	useEffect(() => () => editor.focus(), [editor]);

	const look = (difference: number) =>
		cut.has(difference)
			? "compare__wash marks__cut"
			: "compare__wash marks__added";

	return (
		<div className="editor__text compare__text marks" onClick={onClose}>
			{body.map((block, index) => draw(block, index, look))}
		</div>
	);
}
