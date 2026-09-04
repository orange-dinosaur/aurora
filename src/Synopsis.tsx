import Outline from "./Outline";
import { TextBox } from "./Field";
import type { FieldsHandle } from "./fields";
import { text } from "./frontmatter";
import type { OutlineHandle } from "./outline";

// The Synopsis tab: what this document is about, in the writer's own words,
// and under it the shape of the document itself.

type Props = FieldsHandle & {
	/** The open document's headings, or null for a folder overview, whose
	 * children are already on screen as cards. */
	outline: OutlineHandle | null;
};

export default function Synopsis({ fields, setField, outline }: Props) {
	return (
		<>
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
		</>
	);
}
