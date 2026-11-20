import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";

/** Shows what went wrong, and a way out, instead of a blank window. */
class Crashed extends React.Component<
	{ children: React.ReactNode },
	{ message: string | null }
> {
	state = { message: null as string | null };

	static getDerivedStateFromError(error: unknown) {
		return {
			message: error instanceof Error ? error.message : String(error),
		};
	}

	render() {
		if (this.state.message === null) {
			return this.props.children;
		}
		return (
			<div role="alert" style={{ padding: "2rem" }}>
				<p>Aurora hit a problem: {this.state.message}</p>
				<button type="button" onClick={() => window.location.reload()}>
					Reload
				</button>
			</div>
		);
	}
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
	<React.StrictMode>
		<Crashed>
			<App />
		</Crashed>
	</React.StrictMode>,
);
