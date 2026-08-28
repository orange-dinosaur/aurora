import Controls from "./Controls";

// The bar under the document title. It is always there, so what can be done to
// the text is visible without having to select anything first.
export default function Toolbar() {
	return (
		<div className="toolbar">
			<Controls />
		</div>
	);
}
