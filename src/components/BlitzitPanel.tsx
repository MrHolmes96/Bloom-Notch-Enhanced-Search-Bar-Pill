import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Check, ExternalLink, Loader2, Plus, RefreshCw, Rocket, Zap } from "lucide-react";
import "./BlitzitPanel.css";

/**
 * Blitzit in the notch: add a task to one of your Blitzit lists, tick off
 * today's tasks, and start Blitz mode in the Blitzit app. Tasks go through
 * Blitzit's own connection for other apps, signed in once in Settings >
 * Integrations; starting Blitz mode works without signing in.
 */

export interface BlitzitStatus {
	connected: boolean;
	server: string;
	tools: string[];
	can_add: boolean;
	can_read_lists: boolean;
	can_read_tasks: boolean;
	can_complete: boolean;
	columns: string[];
	/** Blitzit's tools Bloom uses for adding, lists, tasks, ticking off. */
	using: (string | null)[];
	/** Each of Blitzit's tools and what it says it does. */
	tool_details: [string, string][];
	/** Why Blitzit couldn't be reached or used. */
	problem: string | null;
	app_found: boolean;
}

interface List {
	id: string;
	name: string;
}

interface Task {
	id: string;
	name: string;
	done: boolean;
	column: string;
	list: string;
}

const LIST_KEY = "bloom-blitzit-list";
const COLUMN_KEY = "bloom-blitzit-column";
const VISIBLE_TASKS = 4;
const TASK_ROW = 28;

function squash(text: string) {
	return text.toLowerCase().replace(/[^a-z0-9]/g, "");
}

/** What shows: today's tasks when Blitzit says which those are, else the open ones. */
function shownTasks(tasks: Task[]) {
	const open = tasks.filter((t) => !t.done);
	const today = open.filter((t) => squash(t.column).includes("today"));
	return today.length > 0 || open.some((t) => t.column) ? today : open;
}

/** Panel height inside the open notch, below its 36 px header. */
export function blitzitPanelHeight(connected: boolean, taskRows: number, message: boolean) {
	let body = 12 + 30 + 8;
	if (connected) body += 34 + 8;
	if (taskRows > 0) body += Math.min(taskRows, VISIBLE_TASKS) * TASK_ROW;
	else if (message) body += 30;
	return body + 12;
}

function readStored<T>(key: string, fallback: T): T {
	try {
		const raw = localStorage.getItem(key);
		return raw ? (JSON.parse(raw) as T) : fallback;
	} catch {
		return fallback;
	}
}

function saveStored(key: string, value: unknown) {
	const text = JSON.stringify(value);
	try {
		localStorage.setItem(key, text);
	} catch {
		/* the backend keeps it too */
	}
	invoke("save_setting", { key, value: text }).catch(() => {});
}

export function BlitzitPanel({
	onPinChange,
	onHeightChange,
	onOpenSettings
}: {
	onPinChange: (pinned: boolean) => void;
	onHeightChange: (height: number) => void;
	onOpenSettings: () => void;
}) {
	const [status, setStatus] = useState<BlitzitStatus | null>(null);
	const [lists, setLists] = useState<List[]>([]);
	const [list, setListState] = useState<List | null>(() => readStored<List | null>(LIST_KEY, null));
	const listRef = useRef(list);
	const setList = (next: List | null) => {
		listRef.current = next;
		setListState(next);
	};
	const [column, setColumn] = useState<string>(() => readStored<string>(COLUMN_KEY, ""));
	const [tasks, setTasks] = useState<Task[]>([]);
	const [draft, setDraft] = useState("");
	const [busy, setBusy] = useState<string | null>(null);
	const [message, setMessage] = useState<string | null>(null);

	const loadTasks = useCallback(async (from: List | null, current: BlitzitStatus | null) => {
		if (!current?.connected || !current.can_read_tasks) return;
		try {
			setTasks(
				await invoke<Task[]>("blitzit_tasks", {
					listId: from?.id ?? null,
					listName: from?.name ?? null
				})
			);
		} catch (e) {
			setMessage(String(e));
		}
	}, []);

	const load = useCallback(async () => {
		setBusy("load");
		setMessage(null);
		try {
			const next = await invoke<BlitzitStatus>("blitzit_status", { check: true });
			setStatus(next);
			if (next.connected && next.can_read_lists) {
				const found = await invoke<List[]>("blitzit_lists").catch((e) => {
					setMessage(String(e));
					return [] as List[];
				});
				setLists(found);
				const current = listRef.current;
				const kept = found.find((l) => l.id === current?.id) ?? found[0] ?? null;
				if (kept && kept.id !== current?.id) {
					setList(kept);
					saveStored(LIST_KEY, kept);
				}
				await loadTasks(kept, next);
			} else if (next.connected) {
				await loadTasks(null, next);
			}
		} catch (e) {
			setMessage(String(e));
		} finally {
			setBusy(null);
		}
	}, [loadTasks]);

	useEffect(() => {
		void load();
		const changed = listen("blitzit-changed", () => void load());
		return () => {
			void changed.then((f) => f());
		};
	}, [load]);

	// Notes like "Added to Work" fade after a few seconds.
	useEffect(() => {
		if (!message) return;
		const timer = window.setTimeout(() => setMessage(null), 6000);
		return () => window.clearTimeout(timer);
	}, [message]);

	const shown = shownTasks(tasks);
	useEffect(() => {
		onHeightChange(
			blitzitPanelHeight(!!status?.connected, shown.length, !!message || !status?.connected)
		);
	}, [status, shown.length, message, onHeightChange]);

	const columns = status?.columns ?? [];
	const chosenColumn =
		columns.find((c) => c === column) ?? columns.find((c) => squash(c).includes("today")) ?? "";

	const add = async () => {
		const title = draft.trim();
		if (!title || busy) return;
		setBusy("add");
		setMessage(null);
		try {
			await invoke("blitzit_add_task", {
				title,
				listId: list?.id ?? null,
				listName: list?.name ?? null,
				column: chosenColumn || null
			});
			setDraft("");
			setMessage(`Added to ${list?.name ?? "Blitzit"}`);
			await loadTasks(list, status);
		} catch (e) {
			setMessage(String(e));
		} finally {
			setBusy(null);
		}
	};

	const complete = async (task: Task) => {
		setTasks((prev) => prev.map((t) => (t.id === task.id ? { ...t, done: true } : t)));
		try {
			await invoke("blitzit_complete_task", {
				taskId: task.id,
				listId: list?.id ?? null,
				listName: list?.name ?? null
			});
		} catch (e) {
			setMessage(String(e));
			setTasks((prev) => prev.map((t) => (t.id === task.id ? { ...t, done: false } : t)));
		}
	};

	const start = async () => {
		setBusy("start");
		setMessage(null);
		try {
			const result = await invoke<string>("blitzit_start");
			if (result !== "started") setMessage("Blitzit is open: press Blitzit now there");
		} catch (e) {
			setMessage(String(e));
		} finally {
			setBusy(null);
		}
	};

	return (
		<div className="blitzit-panel" onClick={(e) => e.stopPropagation()}>
			<div className="blitzit-top">
				<Zap className="blitzit-mark" />
				<span className="blitzit-title">Blitzit</span>
				{status?.connected && (
					<button
						type="button"
						className="blitzit-icon"
						title="Refresh"
						disabled={!!busy}
						onClick={() => void load()}
					>
						<RefreshCw className={busy === "load" ? "blitzit-spin" : ""} />
					</button>
				)}
				<button
					type="button"
					className="blitzit-icon"
					title="Open Blitzit"
					onClick={() => void invoke("blitzit_open").catch((e) => setMessage(String(e)))}
				>
					<ExternalLink />
				</button>
				<button
					type="button"
					className="blitzit-start"
					title="Start Blitz mode in the Blitzit app"
					disabled={busy === "start"}
					onClick={() => void start()}
				>
					{busy === "start" ? <Loader2 className="blitzit-spin" /> : <Rocket />}
					Blitz now
				</button>
			</div>

			{status?.connected && !status.can_add && (
				<button
					type="button"
					className="blitzit-message is-action is-warning"
					onClick={onOpenSettings}
				>
					{status.problem
						? `Blitzit: ${status.problem}`
						: status.tool_details.length > 0
							? "Bloom didn't recognise Blitzit's add-task action. Pick it in Settings > Integrations > Blitzit"
							: "Checking Blitzit…"}
				</button>
			)}

			{status?.connected && status.can_add && (
				<div className="blitzit-add">
					<input
						className="blitzit-input"
						value={draft}
						placeholder={list ? `Add a task to ${list.name}…` : "Add a task…"}
						disabled={busy === "add"}
						onChange={(e) => setDraft(e.target.value)}
						onFocus={() => onPinChange(true)}
						onBlur={() => onPinChange(false)}
						onKeyDown={(e) => {
							e.stopPropagation();
							if (e.key === "Enter") void add();
							if (e.key === "Escape") (e.target as HTMLInputElement).blur();
						}}
					/>
					{lists.length > 0 && (
						<select
							className="blitzit-select"
							value={list?.id ?? ""}
							title="List"
							onChange={(e) => {
								const next = lists.find((l) => l.id === e.target.value) ?? null;
								setList(next);
								saveStored(LIST_KEY, next);
								void loadTasks(next, status);
							}}
						>
							{lists.map((l) => (
								<option key={l.id} value={l.id}>
									{l.name}
								</option>
							))}
						</select>
					)}
					{columns.length > 1 && (
						<select
							className="blitzit-select"
							value={chosenColumn}
							title="Column"
							onChange={(e) => {
								setColumn(e.target.value);
								saveStored(COLUMN_KEY, e.target.value);
							}}
						>
							{columns.map((c) => (
								<option key={c} value={c}>
									{c
										.replace(/_/g, " ")
										.toLowerCase()
										.replace(/^./, (m) => m.toUpperCase())}
								</option>
							))}
						</select>
					)}
					<button
						type="button"
						className="blitzit-icon"
						title="Add"
						disabled={!draft.trim() || busy === "add"}
						onClick={() => void add()}
					>
						{busy === "add" ? <Loader2 className="blitzit-spin" /> : <Plus />}
					</button>
				</div>
			)}

			{shown.length > 0 && (
				<div className="blitzit-tasks" style={{ maxHeight: VISIBLE_TASKS * TASK_ROW }}>
					{shown.map((task) => (
						<div key={task.id} className={`blitzit-task${task.done ? " is-done" : ""}`}>
							<button
								type="button"
								className="blitzit-check"
								title={
									status?.can_complete ? "Mark done" : "Blitzit can't tick tasks off from here"
								}
								disabled={!status?.can_complete || task.done}
								onClick={() => void complete(task)}
							>
								{task.done && <Check />}
							</button>
							<span className="blitzit-task-name">{task.name}</span>
						</div>
					))}
				</div>
			)}

			{shown.length === 0 &&
				(status && !status.connected ? (
					<button type="button" className="blitzit-message is-action" onClick={onOpenSettings}>
						Connect your Blitzit account in Settings to add tasks from here
					</button>
				) : (
					message && <div className="blitzit-message">{message}</div>
				))}
			{shown.length > 0 && message && (
				<div className="blitzit-toast" role="status">
					{message}
				</div>
			)}
		</div>
	);
}
