import { useRef, useState } from "react";
import {
	Check,
	Cloud,
	CloudOff,
	ExternalLink,
	ListTodo,
	Loader2,
	Maximize2,
	Minimize2,
	Plus,
	RefreshCw,
	Rocket,
	SquareKanban
} from "lucide-react";
import {
	ALL_LISTS,
	boardColumnOf,
	columnsOf,
	DONE_KEY,
	isToday,
	listColor,
	listLetter,
	syncedText,
	type Board,
	type Column,
	type List,
	type Task
} from "./blitzitModel";
import "./BlitzitBoard.css";

/**
 * Blitzit's board in the notch, laid out like Blitzit's own: the lists on
 * the left (All Lists first), the list's name at the top with the view
 * switch, and its columns (Backlog, This Week, Today) side by side, each
 * with "Add task" under its name and, like Blitzit, how much of it is done.
 * Tasks are dragged from one column to another; Blitz Now is at the bottom.
 */

interface DragState {
	task: Task;
	x: number;
	y: number;
	width: number;
	grabX: number;
	grabY: number;
	over: string | null;
	/** Where in that column it would land (among the other cards there). */
	index: number;
}

const COLLAPSED_KEY = "bloom-blitzit-collapsed";

function readCollapsed(): string[] {
	try {
		const raw = localStorage.getItem(COLLAPSED_KEY);
		const parsed: unknown = raw ? JSON.parse(raw) : [];
		return Array.isArray(parsed) ? parsed.filter((v): v is string => typeof v === "string") : [];
	} catch {
		return [];
	}
}

export function BlitzitBoard({
	board,
	list,
	syncing,
	busy,
	message,
	onChooseList,
	onAdd,
	onComplete,
	onMove,
	onReorder,
	onSync,
	onList,
	onOpen,
	onStart,
	onPinChange,
	onOpenSettings
}: {
	board: Board | null;
	list: List | null;
	syncing: boolean;
	busy: string | null;
	message: string | null;
	onChooseList: (list: List) => void;
	onAdd: (title: string, column: Column | null) => Promise<boolean>;
	onComplete: (task: Task) => void;
	onMove: (task: Task, column: Column) => void;
	onReorder: (task: Task, before: Task | null, after: Task | null) => void;
	onSync: () => void;
	onList: () => void;
	onOpen: () => void;
	onStart: () => void;
	onPinChange: (pinned: boolean) => void;
	onOpenSettings: () => void;
}) {
	const [addingIn, setAddingIn] = useState<string | null>(null);
	const [draft, setDraft] = useState("");
	const [collapsed, setCollapsed] = useState<string[]>(readCollapsed);
	const [drag, setDrag] = useState<DragState | null>(null);
	const dragRef = useRef<DragState | null>(null);
	const columnsRef = useRef<HTMLDivElement | null>(null);
	const pressRef = useRef<{ task: Task; x: number; y: number; card: HTMLElement } | null>(null);

	const columns = columnsOf(board, true);
	const keyOf = (column: Column) => (column.done ? DONE_KEY : column.key);
	const tasks = board?.tasks ?? [];
	const lists = board?.lists ?? [];
	const all = list?.id === ALL_LISTS;
	const allTotal = lists.reduce((sum, l) => sum + (l.open ?? 0), 0);
	// "All Lists" wears the first list's mark, as in Blitzit.
	const markOf = (l: List | null) => (l?.id === ALL_LISTS ? (lists[0] ?? l) : l);

	const tasksIn = (column: Column) => {
		const here = tasks.filter((t) => boardColumnOf(t, columns) === column);
		if (column.done) return here;
		// Open tasks first (in Blitzit's order), then the ones done.
		return [...here.filter((t) => !t.done), ...here.filter((t) => t.done)];
	};

	const toggleCollapsed = (key: string) => {
		setCollapsed((prev) => {
			const next = prev.includes(key) ? prev.filter((k) => k !== key) : [...prev, key];
			try {
				localStorage.setItem(COLLAPSED_KEY, JSON.stringify(next));
			} catch {
				/* only remembered while Bloom runs */
			}
			return next;
		});
	};

	const submit = async (column: Column) => {
		const title = draft.trim();
		if (!title) return;
		if (await onAdd(title, column)) setDraft("");
	};

	// ---------------------------------------------------------------- dragging

	const scaleOf = () => {
		const el = columnsRef.current;
		if (!el || !el.offsetWidth) return 1;
		return el.getBoundingClientRect().width / el.offsetWidth;
	};

	/** The column under the pointer, and where among its cards it is. */
	const dropAt = (x: number, y: number, dragged: Task) => {
		const el = document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-column]");
		if (!el) return { over: null, index: 0 };
		const cards = Array.from(el.querySelectorAll<HTMLElement>("[data-task]")).filter(
			(c) => c.dataset.task !== dragged.id
		);
		let index = cards.length;
		for (let i = 0; i < cards.length; i++) {
			const r = cards[i].getBoundingClientRect();
			if (y < r.top + r.height / 2) {
				index = i;
				break;
			}
		}
		return { over: el.dataset.column ?? null, index };
	};

	const position = (clientX: number, clientY: number) => {
		const el = columnsRef.current;
		const root = el?.getBoundingClientRect();
		if (!el || !root) return { x: 0, y: 0 };
		const k = scaleOf();
		return {
			x: (clientX - root.left) / k + el.scrollLeft,
			y: (clientY - root.top) / k + el.scrollTop
		};
	};

	const onCardDown = (e: React.PointerEvent<HTMLElement>, task: Task) => {
		if (e.button !== 0 || !(board?.can_move || board?.can_reorder)) return;
		if ((e.target as HTMLElement).closest("button")) return;
		pressRef.current = { task, x: e.clientX, y: e.clientY, card: e.currentTarget };
		e.currentTarget.setPointerCapture(e.pointerId);
	};

	const onCardMove = (e: React.PointerEvent<HTMLElement>) => {
		const press = pressRef.current;
		if (!press) return;
		if (!dragRef.current && Math.hypot(e.clientX - press.x, e.clientY - press.y) < 5) return;
		const k = scaleOf();
		let next: DragState;
		if (!dragRef.current) {
			const card = press.card.getBoundingClientRect();
			next = {
				task: press.task,
				...position(e.clientX, e.clientY),
				width: card.width / k,
				grabX: (press.x - card.left) / k,
				grabY: (press.y - card.top) / k,
				...dropAt(e.clientX, e.clientY, press.task)
			};
			onPinChange(true);
		} else {
			next = {
				...dragRef.current,
				...position(e.clientX, e.clientY),
				...dropAt(e.clientX, e.clientY, press.task)
			};
		}
		dragRef.current = next;
		setDrag(next);
	};

	const endDrag = (drop: boolean) => {
		const current = dragRef.current;
		pressRef.current = null;
		dragRef.current = null;
		setDrag(null);
		if (!current) return;
		onPinChange(false);
		if (!drop || current.over === null) return;
		const target = columns.find((c) => keyOf(c) === current.over);
		if (!target) return;
		const task = current.task;
		const from = boardColumnOf(task, columns);
		const others = tasksIn(target).filter((t) => t.id !== task.id);
		const before = others[current.index] ?? null;
		const after = current.index > 0 ? (others[current.index - 1] ?? null) : null;
		if (target.done) {
			if (!task.done) onComplete(task);
			return;
		}
		if (from !== target || task.done) {
			if (!board?.can_move) return;
			onMove(task, target);
		} else {
			// Same column: only a new place in it.
			const now = tasksIn(target).findIndex((t) => t.id === task.id);
			if (now === current.index) return;
		}
		// Placed where it was dropped (Blitzit keeps each column's order).
		if (board?.can_reorder && (before || after)) onReorder(task, before, after);
	};

	// ---------------------------------------------------------------- parts

	const avatar = (l: List | null, big = false) => {
		const mark = markOf(l);
		return (
			<span
				className={`bz-avatar${big ? " is-big" : ""}${mark?.emoji ? " is-emoji" : ""}`}
				style={mark?.emoji ? undefined : { background: listColor(mark) }}
			>
				{mark?.emoji || listLetter(mark)}
			</span>
		);
	};

	const card = (task: Task) => (
		<div
			key={task.id}
			data-task={task.id}
			className={`bz-card${task.done ? " is-done" : ""}${drag?.task.id === task.id ? " is-lifted" : ""}${board?.can_move || board?.can_reorder ? " can-drag" : ""}`}
			onPointerDown={(e) => onCardDown(e, task)}
			onPointerMove={onCardMove}
			onPointerUp={() => endDrag(true)}
			onPointerCancel={() => endDrag(false)}
		>
			<button
				type="button"
				className="bz-check"
				title={board?.can_complete ? "Mark done" : "Blitzit can't tick tasks off from here"}
				disabled={!board?.can_complete || task.done}
				onClick={(e) => {
					e.stopPropagation();
					onComplete(task);
				}}
			>
				{task.done && <Check />}
			</button>
			<span className="bz-card-name">{task.name}</span>
			{task.pending && (
				<span className="bz-pending" title="Waiting to reach Blitzit">
					<CloudOff />
				</span>
			)}
			{all && (
				<span className="bz-card-list" title={lists.find((l) => l.id === task.list_id)?.name}>
					{avatar(lists.find((l) => l.id === task.list_id) ?? null)}
				</span>
			)}
		</div>
	);

	const addRow = (column: Column) =>
		addingIn === column.key ? (
			<div className="bz-add-input">
				<input
					autoFocus
					value={draft}
					placeholder={`Add a task to ${column.label}`}
					disabled={busy === "add"}
					onChange={(e) => setDraft(e.target.value)}
					onFocus={() => onPinChange(true)}
					onBlur={() => {
						onPinChange(false);
						if (!draft.trim()) setAddingIn(null);
					}}
					onKeyDown={(e) => {
						e.stopPropagation();
						if (e.key === "Enter") void submit(column);
						if (e.key === "Escape") {
							setDraft("");
							setAddingIn(null);
						}
					}}
				/>
				{busy === "add" && <Loader2 className="bz-spin" />}
			</div>
		) : (
			<button
				type="button"
				className="bz-add"
				onClick={() => {
					setDraft("");
					setAddingIn(column.key);
				}}
			>
				<Plus /> Add task
			</button>
		);

	return (
		<div className={`bz${drag ? " is-dragging" : ""}`} onClick={(e) => e.stopPropagation()}>
			<aside className="bz-side">
				<div className="bz-side-label">Lists</div>
				<div className="bz-side-lists">
					{lists.length > 0 && (
						<button
							type="button"
							className={`bz-side-list${all ? " is-on" : ""}`}
							onClick={() => onChooseList({ id: ALL_LISTS, name: "All Lists" })}
						>
							{avatar({ id: ALL_LISTS, name: "All Lists" })}
							<span className="bz-side-name">All Lists</span>
							<span className="bz-side-count">{allTotal}</span>
						</button>
					)}
					{lists.map((l) => (
						<button
							key={l.id}
							type="button"
							className={`bz-side-list${l.id === list?.id ? " is-on" : ""}`}
							onClick={() => onChooseList(l)}
						>
							{avatar(l)}
							<span className="bz-side-name">{l.name}</span>
							<span className="bz-side-count">{l.open ?? 0}</span>
						</button>
					))}
				</div>
				<button
					type="button"
					className={`bz-side-sync${board?.offline ? " is-offline" : ""}`}
					title="Sync now"
					disabled={syncing}
					onClick={onSync}
				>
					{syncing ? (
						<RefreshCw className="bz-spin" />
					) : board?.offline ? (
						<CloudOff />
					) : (
						<Cloud />
					)}
					<span>{message ?? syncedText(board)}</span>
				</button>
			</aside>

			<main className="bz-main">
				<header className="bz-head">
					{avatar(list, true)}
					<h2 className="bz-title">{list?.name ?? "Blitzit"}</h2>
					<span className="bz-spacer" />
					<div className="bz-views" role="group" aria-label="View">
						<button type="button" onClick={onList} title="List: add tasks to the list you're on">
							<ListTodo /> List
						</button>
						<button type="button" className="is-on" title="Board">
							<SquareKanban /> Board
						</button>
					</div>
					<button type="button" className="bz-icon" title="Open Blitzit" onClick={onOpen}>
						<ExternalLink />
					</button>
				</header>

				{board?.problem && (
					<button type="button" className="bz-problem" onClick={onOpenSettings}>
						Blitzit: {board.problem}
					</button>
				)}

				<div className="bz-columns" ref={columnsRef}>
					{columns.map((column) => {
						const id = keyOf(column);
						const here = tasksIn(column);
						const done = here.filter((t) => t.done).length;
						const folded = collapsed.includes(id);
						const today = isToday(column);
						const progress = !column.done && !/backlog/i.test(column.key);
						const canAdd = !column.done && board?.can_add;
						// Where a dragged card would land.
						const shown = drag ? here.filter((t) => t.id !== drag.task.id) : here;
						const lineAt = drag?.over === id ? drag.index : -1;
						return (
							<section
								key={id || "none"}
								data-column={id}
								className={`bz-col${today ? " is-today" : ""}${column.done ? " is-done" : ""}${folded ? " is-folded" : ""}${drag?.over === id ? " is-over" : ""}`}
							>
								<div className="bz-col-head">
									<h3>{column.label}</h3>
									{!folded && canAdd && (
										<button
											type="button"
											className="bz-icon"
											title="Add task"
											onClick={() => {
												setDraft("");
												setAddingIn(column.key);
											}}
										>
											<Plus />
										</button>
									)}
									<button
										type="button"
										className="bz-icon"
										title={folded ? "Expand" : "Collapse"}
										onClick={() => toggleCollapsed(id)}
									>
										{folded ? <Maximize2 /> : <Minimize2 />}
									</button>
								</div>
								{!folded && (
									<>
										{progress && (
											<div className="bz-progress">
												<div className="bz-progress-track">
													<div
														className="bz-progress-fill"
														style={{ width: here.length ? `${(done / here.length) * 100}%` : 0 }}
													/>
												</div>
												<span>
													{done}/{here.length} done
												</span>
											</div>
										)}
										{canAdd && addRow(column)}
										<div className="bz-cards">
											{(drag ? [...shown.slice(0, Math.max(lineAt, 0)), null, ...shown.slice(Math.max(lineAt, 0))] : here).map(
												(task, i) =>
													task ? (
														card(task)
													) : lineAt >= 0 ? (
														<div key={`line-${i}`} className="bz-drop-line" />
													) : null
											)}
										</div>
									</>
								)}
							</section>
						);
					})}
					{drag && (
						<div
							className="bz-card bz-ghost"
							style={{ left: drag.x - drag.grabX, top: drag.y - drag.grabY, width: drag.width }}
						>
							<span className="bz-check" />
							<span className="bz-card-name">{drag.task.name}</span>
						</div>
					)}
				</div>

				<footer className="bz-foot">
					<button
						type="button"
						className="bz-blitz"
						title="Start Blitz mode in the Blitzit app"
						disabled={busy === "start"}
						onClick={onStart}
					>
						{busy === "start" ? <Loader2 className="bz-spin" /> : <Rocket />}
						Blitz Now
					</button>
				</footer>
			</main>
		</div>
	);
}
