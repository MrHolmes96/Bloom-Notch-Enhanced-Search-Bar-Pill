import { Component, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";

/**
 * Keeps a fault in one notch page inside that page. Without it, an error
 * while drawing (say, the note editor) takes the whole notch down with it.
 * The page shows a short message and a button to draw it again; the error
 * goes into crash.log, so it is in Export Diagnostics.
 */
interface Props {
	/** The page's name, for the message and the log. */
	page: string;
	children: ReactNode;
}

interface State {
	failed: boolean;
}

export class PanelBoundary extends Component<Props, State> {
	state: State = { failed: false };

	static getDerivedStateFromError(): State {
		return { failed: true };
	}

	componentDidCatch(error: unknown) {
		const message = error instanceof Error ? `${error.message} ${error.stack ?? ""}` : String(error);
		console.error(`[${this.props.page}]`, error);
		invoke("note_page_error", { page: this.props.page, message: message.slice(0, 1500) }).catch(
			() => {}
		);
	}

	render() {
		if (!this.state.failed) return this.props.children;
		return (
			<div className="panel-fault" onClick={(e) => e.stopPropagation()}>
				<span>{this.props.page} ran into a problem. Your data is safe.</span>
				<button type="button" onClick={() => this.setState({ failed: false })}>
					Try again
				</button>
			</div>
		);
	}
}
