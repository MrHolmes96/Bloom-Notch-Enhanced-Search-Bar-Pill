import { useState } from "react";
import { Send } from "lucide-react";

/**
 * A notification's own buttons and reply boxes, as on Windows' pop-up.
 * Clicks here never count as a click on the notification itself.
 */

export interface ToastButton {
	label: string;
	/** The reply box whose text this button sends. */
	input: string | null;
}

export interface ToastInput {
	id: string;
	placeholder: string;
}

interface Props {
	buttons: ToastButton[];
	inputs: ToastInput[];
	onPress: (button: number, typed: Record<string, string>) => void;
	/** The reply box got or lost the keyboard. */
	onTyping?: (typing: boolean) => void;
}

export function ToastActions({ buttons, inputs, onPress, onTyping }: Props) {
	const [typed, setTyped] = useState<Record<string, string>>({});
	if (buttons.length === 0 && inputs.length === 0) return null;

	// Enter in a reply box presses the button that sends it.
	const sendFrom = (inputId: string) => {
		const index = buttons.findIndex((b) => b.input === inputId);
		if (index >= 0 && (typed[inputId] ?? "").trim()) onPress(index, typed);
	};
	const replyButton = (inputId: string) => buttons.findIndex((b) => b.input === inputId);

	return (
		<div className="notify-actions-area" onClick={(e) => e.stopPropagation()}>
			{inputs.map((input) => (
				<div key={input.id} className="notify-reply">
					<input
						type="text"
						value={typed[input.id] ?? ""}
						placeholder={input.placeholder || "Type a reply"}
						onMouseDown={() => onTyping?.(true)}
						onFocus={() => onTyping?.(true)}
						onBlur={() => onTyping?.(false)}
						onChange={(e) => setTyped((t) => ({ ...t, [input.id]: e.target.value }))}
						onKeyDown={(e) => {
							e.stopPropagation();
							if (e.key === "Enter") sendFrom(input.id);
						}}
					/>
					{replyButton(input.id) >= 0 && (
						<button
							type="button"
							className="notify-reply-send"
							title={buttons[replyButton(input.id)].label}
							disabled={!(typed[input.id] ?? "").trim()}
							onClick={() => sendFrom(input.id)}
						>
							<Send size={13} />
						</button>
					)}
				</div>
			))}
			{buttons.some((b) => !b.input || !inputs.some((i) => i.id === b.input)) && (
				<div className="notify-actions">
					{buttons.map((button, index) =>
						button.input && inputs.some((i) => i.id === button.input) ? null : (
							<button
								key={index}
								type="button"
								className="notify-action"
								onClick={() => onPress(index, typed)}
							>
								{button.label}
							</button>
						)
					)}
				</div>
			)}
		</div>
	);
}
