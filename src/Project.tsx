import Sidebar from "./Sidebar";

type Props = {
	name: string;
	root: string;
	onClose: () => void;
};

export default function Project({ name, root, onClose }: Props) {
	return (
		<section className="project">
			<header className="project__header">
				<h1 className="project__title">{name}</h1>
				<button
					type="button"
					className="project__close"
					onClick={onClose}
				>
					Close project
				</button>
			</header>

			<div className="project__body">
				<Sidebar root={root} />
				<div className="project__main">
					<p className="project__path">
						<code>{root}</code>
					</p>
					<p className="project__empty">
						Choosing a document to read comes next.
					</p>
				</div>
			</div>
		</section>
	);
}
