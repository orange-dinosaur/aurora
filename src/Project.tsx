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
			<p className="project__path">
				<code>{root}</code>
			</p>
			<p className="project__empty">
				Your manuscript will appear here once Aurora can read it.
			</p>
		</section>
	);
}
