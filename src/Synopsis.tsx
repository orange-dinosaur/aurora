import TextBox from "./Field";
import type { FieldsHandle } from "./fields";
import { text } from "./frontmatter";

// The Synopsis tab: what this document is about, in the writer's own words. The
// structure below it, headings for a document and children for a folder, joins
// it later.

export default function Synopsis({ fields, setField }: FieldsHandle) {
	return (
		<TextBox
			label="Synopsis"
			value={text(fields, "synopsis")}
			placeholder="What happens here."
			rows={7}
			onChange={(value) =>
				setField("synopsis", value === "" ? null : value)
			}
		/>
	);
}
