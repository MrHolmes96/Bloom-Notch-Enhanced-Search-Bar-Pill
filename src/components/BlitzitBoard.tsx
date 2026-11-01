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
	columnFor,
	columnsOf,
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

	const columns = columnsOf(board, false);
	const tasks = board?.tasks ?? [];
	const lists = board?.lists ?? [];
	const all = list?.id === ALL_LISTS;
	const allTotal = lists.reduce((sum, l) => sum + (l.open ?? 0), 0);
	// "All Lists" wears the first list's mark, as in Blitzit.
	const markOf = (l: List | null) => (l?.id === ALL_LISTS ? (lists[0] ?? l) : l);

	const tasksIn = (column: Column) => {
		const here = tasks.filter((t) => columnFor(columns, t.column) === column);
		// Open tasks first, then the ones done.
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

	const columnAt = (x: number, y: number) => {
		const el = document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-column]");
		return el?.dataset.column ?? null;
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
		if (e.button !== 0 || task.done || !board?.can_move) return;
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
				over: columnAt(e.clientX, e.clientY)
			};
			onPinChange(true);
		} else {
			next = { ...dragRef.current, ...position(e.clientX, e.clientY), over: columnAt(e.clientX, e.clientY) };
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
		const target = columns.find((c) => c.key === current.over);
		if (target && columnFor(columns, current.task.column) !== target) onMove(current.task, target);
	};

	// ---------------------------------------------------------------- parts

	const avatar = (l: List | null, big = false) => {
		const mark = markOf(l);
		return (
			<span className={`bz-avatar${big ? " is-big" : ""}`} style={{ background: listColor(mark) }}>
				{listLetter(mark)}
			</span>
		);
	};

	const card = (task: Task) => (
		<div
			key={task.id}
			className={`bz-card${task.done ? " is-done" : ""}${drag?.task.id === task.id ? " is-lifted" : ""}${board?.can_move && !task.done ? " can-drag" : ""}`}
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
						const here = tasksIn(column);
						const done = here.filter((t) => t.done).length;
						const folded = collapsed.includes(column.key);
						const today = isToday(column);
						const progress = !/backlog/i.test(column.key);
						return (
							<section
								key={column.key || "none"}
								data-column={column.key}
								className={`bz-col${today ? " is-today" : ""}${folded ? " is-folded" : ""}${drag?.over === column.key ? " is-over" : ""}`}
							>
								<div className="bz-col-head">
									<h3>{column.label}</h3>
									{!folded && board?.can_add && (
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
										onClick={() => toggleCollapsed(column.key)}
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
										{board?.can_add && addRow(column)}
										<div className="bz-cards">{here.map(card)}</div>
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
