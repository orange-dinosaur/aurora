import Outline from "./Outline";
import { TextBox } from "./Field";
import type { FieldsHandle } from "./fields";
import { text } from "./frontmatter";
import type { OutlineHandle } from "./lib/outline";
import { TakeMarks, type MarksHandle } from "./TakeStrip";

// The Synopsis tab: what this document is about, in the writer's own words,
// under it the shape of the document itself, and then its takes.

type Props = FieldsHandle & {
	/** The open document's headings, or null for a folder overview, whose
	 * children are already on screen as cards. */
	outline: OutlineHandle | null;
	/** The open document's takes, or null when it has none. */
	marks: MarksHandle | null;
};

export default function Synopsis({ fields, setField, outline, marks }: Props) {
	return (
		<div className="synopsis">
			<TextBox
				label="Synopsis"
				value={text(fields, "synopsis")}
				placeholder="What happens here."
				rows={7}
				onChange={(value) =>
					setField("synopsis", value === "" ? null : value)
				}
			/>
			{outline !== null && <Outline {...outline} />}
			{marks !== null && <TakeMarks {...marks} />}
		</div>
	);
}
