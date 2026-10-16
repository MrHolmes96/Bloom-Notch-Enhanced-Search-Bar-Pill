import { useState } from "react";
import { Check, ListTodo, StickyNote, X } from "lucide-react";
import "./NotesPanel.css";

/**
 * Quick notes and to-dos in the notch, for things that don't belong in a
 * calendar. Newest first; to-dos can be ticked off and cleared.
 */

export interface NoteItem {
	id: string;
	text: string;
	todo: boolean;
	done: boolean;
	created: number;
}

export const NOTES_KEY = "bloom-notch-notes";
/** Rows shown before the list scrolls. */
export const NOTES_VISIBLE_ROWS = 5;
export const NOTES_ROW_HEIGHT = 30;

export function parseNotes(raw: unknown): NoteItem[] {
	if (Array.isArray(raw)) return raw as NoteItem[];
	if (typeof raw !== "string" || !raw) return [];
	try {
		const parsed = JSON.parse(raw);
		return Array.isArray(parsed)
			? parsed.filter((n) => n && typeof n.id === "string" && typeof n.text === "string")
			: [];
	} catch {
		return [];
	}
}

/** Panel height inside the open notch, below its 36 px header. */
export function notesPanelHeight(items: NoteItem[]) {
	const rows = Math.min(items.length, NOTES_VISIBLE_ROWS);
	const list = items.length === 0 ? 34 : rows * NOTES_ROW_HEIGHT;
	const clear = items.some((n) => n.done) ? 28 : 0;
	return 12 + 34 + 8 + list + clear + 12;
}

interface NotesPanelProps {
	items: NoteItem[];
	onChange: (next: NoteItem[]) => void;
	/** Keeps the notch open while typing. */
	onPinChange: (pinned: boolean) => void;
}

export function NotesPanel({ items, onChange, onPinChange }: NotesPanelProps) {
	const [draft, setDraft] = useState("");
	const [todo, setTodo] = useState(() => localStorage.getItem("bloom-notch-notes-kind") !== "note");
	const [editing, setEditing] = useState<string | null>(null);
	const [editText, setEditText] = useState("");

	const add = () => {
		const text = draft.trim();
		if (!text) return;
		onChange([
			{
				id: `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 7)}`,
				text,
				todo,
				done: false,
				created: Date.now()
			},
			...items
		]);
		setDraft("");
	};

	const update = (id: string, change: Partial<NoteItem>) =>
		onChange(items.map((n) => (n.id === id ? { ...n, ...change } : n)));

	const finishEdit = (save: boolean) => {
		if (save && editing) {
			const text = editText.trim();
			if (text) update(editing, { text });
		}
		setEditing(null);
		onPinChange(false);
	};

	const doneCount = items.filter((n) => n.done).length;

	return (
		<div className="notes-panel" onClick={(e) => e.stopPropagation()}>
			<div className="notes-input-row">
				<button
					type="button"
					className="notes-kind"
					title={todo ? "Adding to-dos (click for notes)" : "Adding notes (click for to-dos)"}
					onClick={() => {
						const next = !todo;
						setTodo(next);
						localStorage.setItem("bloom-notch-notes-kind", next ? "todo" : "note");
					}}
				>
					{todo ? <ListTodo size={15} /> : <StickyNote size={15} />}
				</button>
				<input
					className="notes-input"
					placeholder={todo ? "Add a to-do…" : "Add a note…"}
					value={draft}
					spellCheck={false}
					onChange={(e) => setDraft(e.target.value)}
					onFocus={() => onPinChange(true)}
					onBlur={() => onPinChange(false)}
					onKeyDown={(e) => {
						e.stopPropagation();
						if (e.key === "Enter") add();
						if (e.key === "Escape") (e.target as HTMLInputElement).blur();
					}}
				/>
			</div>

			{items.length === 0 ? (
				<div className="notes-empty">Nothing here yet. Type above and press Enter</div>
			) : (
				<div className="notes-list" style={{ maxHeight: NOTES_VISIBLE_ROWS * NOTES_ROW_HEIGHT }}>
					{items.map((item) => (
						<div
							key={item.id}
							className={`notes-row${item.done ? " done" : ""}`}
							style={{ minHeight: NOTES_ROW_HEIGHT }}
						>
							{item.todo ? (
								<button
									type="button"
									className={`notes-check${item.done ? " checked" : ""}`}
									title={item.done ? "Mark as not done" : "Mark as done"}
									onClick={() => update(item.id, { done: !item.done })}
								>
									{item.done && <Check size={11} strokeWidth={3} />}
								</button>
							) : (
								<span className="notes-dot" />
							)}
							{editing === item.id ? (
								<input
									className="notes-edit"
									autoFocus
									value={editText}
									onChange={(e) => setEditText(e.target.value)}
									onFocus={() => onPinChange(true)}
									onBlur={() => finishEdit(true)}
									onKeyDown={(e) => {
										e.stopPropagation();
										if (e.key === "Enter") finishEdit(true);
										if (e.key === "Escape") finishEdit(false);
									}}
								/>
							) : (
								<span
									className="notes-text"
									title="Double-click to edit"
									onDoubleClick={() => {
										setEditing(item.id);
										setEditText(item.text);
									}}
								>
									{item.text}
								</span>
							)}
							<button
								type="button"
								className="notes-delete"
								title="Delete"
								onClick={() => onChange(items.filter((n) => n.id !== item.id))}
							>
								<X size={12} />
							</button>
						</div>
					))}
				</div>
			)}

			{doneCount > 0 && (
				<button
					type="button"
					className="notes-clear"
					onClick={() => onChange(items.filter((n) => !n.done))}
				>
					Clear {doneCount} done
				</button>
			)}
		</div>
	);
}
