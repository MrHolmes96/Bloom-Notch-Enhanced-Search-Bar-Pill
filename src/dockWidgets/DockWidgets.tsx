import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type RefObject } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Check, Layers, Plus, X } from "lucide-react";
import { EdgeSurface } from "../components/EdgeSurface";
import { useSettingsSync } from "../hooks/useSettingsSync";
import { useClock24h } from "../hooks/useClock24h";
import { useWeather } from "../hooks/useWeather";
import { useMedia, useMetrics, useNow, useTodayTasks } from "./data";
import {
	LAYOUT_KEY,
	WIDGETS,
	cellAt,
	fits,
	gridFor,
	newId,
	placeOf,
	readLayout,
	shelves,
	widgetInfo,
	type Grid,
	type Layout,
	type Placed,
	type Side,
	type WidgetKind,
	type WidgetSize
} from "./model";
import { Widget, ZONES, readFocus, writeFocus, type WidgetData } from "./widgets";
import "./DockWidgets.css";

/**
 * Widgets in the strips beside the dock (see model.ts for the layout).
 *
 * Each side shows its widgets on shelves like the dock: neighbours share one,
 * attached to the screen's bottom edge. They come and go with the dock (hidden
 * over full-screen apps, and in smart mode while the window in front covers
 * that side, until the dock is called up). Right-click a widget for its menu;
 * Edit widgets (there, or in the desktop's menu beside the dock) shows the grid
 * and a picker to drag widgets from. Dropping a widget onto another makes a
 * group: the mouse wheel flips through it, and it can turn on its own.
 */

interface Props {
	dockRef: RefObject<HTMLDivElement | null>;
	scale: number;
	/** The dock is out of sight (hidden, not ready, or a full-screen app). */
	hidden: boolean;
	/** Smart mode with nothing calling the dock up: a covered side hides. */
	stepAside: boolean;
}

interface Drag {
	/** From the picker (a new widget) or an item being moved. */
	kind?: WidgetKind;
	itemId?: string;
	fromSide?: Side;
	size: WidgetSize;
	x: number;
	y: number;
	moved: boolean;
}

interface Menu {
	side: Side;
	itemId: string;
	x: number;
	y: number;
}

const ROTATIONS = [0, 5, 10, 30, 60];
const FOCUS_LENGTHS = [15, 25, 45, 60];

export function DockWidgets({ dockRef, scale, hidden, stepAside }: Props) {
	const [layout, setLayout] = useState<Layout>(() => readLayout(localStorage.getItem(LAYOUT_KEY)));
	const [editing, setEditing] = useState(false);
	const [covered, setCovered] = useState<[boolean, boolean]>([false, false]);
	const [dock, setDock] = useState<{ left: number; right: number; height: number } | null>(null);
	const [width, setWidth] = useState(window.innerWidth);
	const [shown, setShown] = useState<Record<string, number>>({});
	const [drag, setDrag] = useState<Drag | null>(null);
	const [menu, setMenu] = useState<Menu | null>(null);
	const pickerRef = useRef<HTMLDivElement | null>(null);
	const menuRef = useRef<HTMLDivElement | null>(null);
	const hovered = useRef<string | null>(null);

	// ---- the saved layout
	useEffect(() => {
		void invoke<Record<string, unknown>>("load_settings")
			.then((s) => {
				const text = s?.[LAYOUT_KEY];
				if (typeof text === "string") setLayout(readLayout(text));
			})
			.catch(() => {});
	}, []);
	useSettingsSync({ [LAYOUT_KEY]: (v) => setLayout(readLayout(typeof v === "string" ? v : JSON.stringify(v ?? ""))) });
	const save = useCallback((next: Layout) => {
		setLayout(next);
		const text = JSON.stringify(next);
		try {
			localStorage.setItem(LAYOUT_KEY, text);
		} catch {}
		void invoke("save_setting", { key: LAYOUT_KEY, value: text }).catch(() => {});
	}, []);

	// ---- where the dock is: the grid hangs off it
	useLayoutEffect(() => {
		const measure = () => {
			const el = dockRef.current;
			setWidth(window.innerWidth);
			if (!el) return;
			const r = el.getBoundingClientRect();
			if (r.width < 40 || r.height < 30) return;
			setDock((prev) =>
				prev && Math.abs(prev.left - r.left) < 0.5 && Math.abs(prev.right - r.right) < 0.5 && Math.abs(prev.height - r.height) < 0.5
					? prev
					: { left: r.left, right: r.right, height: r.height }
			);
		};
		measure();
		const observer = new ResizeObserver(measure);
		if (dockRef.current) observer.observe(dockRef.current);
		window.addEventListener("resize", measure);
		const timer = window.setInterval(measure, 1000);
		return () => {
			observer.disconnect();
			window.removeEventListener("resize", measure);
			window.clearInterval(timer);
		};
	}, [dockRef]);
	const grid: Grid | null = useMemo(() => (dock ? gridFor(dock, width, scale) : null), [dock, width, scale]);

	// ---- covered by the window in front (smart mode), and edit requests
	useEffect(() => {
		const a = listen<[boolean, boolean]>("dock-widgets-covered", (e) => setCovered(e.payload));
		const b = listen("dock-widgets-edit", () => setEditing(true));
		const c = () => setEditing(true);
		window.addEventListener("dw-edit", c);
		return () => {
			void a.then((f) => f());
			void b.then((f) => f());
			window.removeEventListener("dw-edit", c);
		};
	}, []);
	const sideHidden = (side: Side) =>
		!editing && (hidden || (stepAside && covered[side === "left" ? 0 : 1]));

	// ---- what the placed widgets need
	const placed = useMemo(
		() => new Set([...layout.left, ...layout.right].flatMap((p) => p.widgets)),
		[layout]
	);
	const metrics = useMetrics(placed);
	const weather = useWeather(placed.has("weather"));
	const media = useMedia(placed.has("media"));
	const today = useTodayTasks(placed.has("tasks"));
	const ticking = ["clock", "worldclock", "focus", "date", "countdown", "progress"].some((k) => placed.has(k as WidgetKind));
	const now = useNow(ticking, 1000);
	const h24 = useClock24h();
	const data: WidgetData = {
		metrics,
		weather: {
			temperature: weather.temperature,
			condition: weather.weatherCondition,
			icon: weather.weatherIcon,
			city: weather.cityName,
			unit: weather.tempUnit === "fahrenheit" ? "F" : "C"
		},
		media,
		today,
		now,
		h24
	};

	// ---- groups: the wheel flips, and a group can turn on its own
	const flip = (item: Placed, by: number) =>
		setShown((s) => ({
			...s,
			[item.id]: ((s[item.id] ?? 0) + by + item.widgets.length) % item.widgets.length
		}));
	useEffect(() => {
		const turning = [...layout.left, ...layout.right].filter((p) => p.widgets.length > 1 && p.rotate > 0);
		if (!turning.length) return;
		const timers = turning.map((item) =>
			window.setInterval(() => {
				if (hovered.current !== item.id) flip(item, 1);
			}, item.rotate * 1000)
		);
		return () => timers.forEach((t) => window.clearInterval(t));
	}, [layout]);

	// ---- where the shelves are: they take clicks (and the dock window's
	// whole lower band while editing, for the picker and dragging)
	const shelfRects = useMemo(() => {
		if (!grid) return [] as { side: Side; items: Placed[]; left: number; width: number }[];
		const out: { side: Side; items: Placed[]; left: number; width: number }[] = [];
		for (const side of ["left", "right"] as Side[]) {
			const items = layout[side].filter((p) => p.cell + (p.size === "large" ? 4 : p.size === "wide" ? 2 : 1) <= grid.cells[side]);
			for (const run of shelves(items)) {
				const places = run.map((p) => placeOf(grid, side, p));
				const left = Math.min(...places.map((p) => p.left)) - grid.pad;
				const right = Math.max(...places.map((p) => p.left + p.width)) + grid.pad;
				out.push({ side, items: run, left, width: right - left });
			}
		}
		return out;
	}, [grid, layout]);

	useEffect(() => {
		if (!dock) return;
		const height = dock.height;
		const rects = shelfRects
			.filter((s) => !sideHidden(s.side))
			.map((s) => ({
				x: Math.round(s.left),
				y: Math.round(window.innerHeight - height),
				width: Math.round(s.width),
				height: Math.round(height)
			}));
		void invoke("set_dock_widget_rects", { rects }).catch(() => {});
	}, [shelfRects, dock, hidden, stepAside, covered, editing]);

	useEffect(() => {
		if (!editing && !menu) return;
		const report = () => {
			if (menu && menuRef.current) {
				const r = menuRef.current.getBoundingClientRect();
				void invoke("set_menu_open", {
					open: true,
					rect: { x: Math.round(r.left), y: Math.round(r.top), width: Math.round(r.width), height: Math.round(r.height) }
				}).catch(() => {});
				return;
			}
			const top = pickerRef.current?.getBoundingClientRect().top ?? window.innerHeight - 260;
			void invoke("set_menu_open", {
				open: true,
				rect: { x: 0, y: Math.round(top - 8), width: window.innerWidth, height: Math.round(window.innerHeight - top + 8) }
			}).catch(() => {});
		};
		const frame = requestAnimationFrame(report);
		// Said again: the dock's own menus closing clear it.
		const timer = window.setInterval(report, 500);
		return () => {
			cancelAnimationFrame(frame);
			window.clearInterval(timer);
		};
	});
	useEffect(() => {
		if (editing || menu) return;
		void invoke("set_menu_open", { open: false, rect: null }).catch(() => {});
	}, [editing, menu]);

	// A press elsewhere (Bloom's mouse hook says) closes the menu and editing.
	useEffect(() => {
		const off = listen("dock-dismiss-menus", () => {
			setMenu(null);
			setEditing(false);
		});
		const onKey = (e: KeyboardEvent) => {
			if (e.key === "Escape") {
				setMenu(null);
				setEditing(false);
			}
		};
		window.addEventListener("keydown", onKey);
		return () => {
			void off.then((f) => f());
			window.removeEventListener("keydown", onKey);
		};
	}, []);

	// ---- dragging (from the picker, or an item while editing)
	const target = (x: number, y: number) => {
		if (!grid || !dock) return null;
		if (y < window.innerHeight - dock.height - 24 * scale) return null;
		const side: Side = x < (dock.left + dock.right) / 2 ? "left" : "right";
		const cell = cellAt(grid, side, x);
		const over = layout[side].find((p) => {
			const place = placeOf(grid, side, p);
			return x >= place.left && x <= place.left + place.width;
		});
		return { side, cell, over };
	};

	useEffect(() => {
		if (!drag) return;
		const move = (e: PointerEvent) =>
			setDrag((d) => (d ? { ...d, x: e.clientX, y: e.clientY, moved: d.moved || Math.abs(e.clientX - d.x) + Math.abs(e.clientY - d.y) > 4 } : d));
		const up = (e: PointerEvent) => {
			const d = drag;
			setDrag(null);
			if (!d || !grid) return;
			const t = target(e.clientX, e.clientY);
			if (!t) return;
			const next: Layout = { version: 1, left: [...layout.left], right: [...layout.right] };
			const source = d.itemId && d.fromSide ? next[d.fromSide].find((p) => p.id === d.itemId) : undefined;
			if (t.over && t.over.id !== d.itemId) {
				// Onto another widget: a group.
				const add = source ? source.widgets : d.kind ? [d.kind] : [];
				const merged = [...t.over.widgets, ...add.filter((k) => !t.over!.widgets.includes(k))];
				next[t.side] = next[t.side].map((p) => (p.id === t.over!.id ? { ...p, widgets: merged } : p));
				if (source && d.fromSide) next[d.fromSide] = next[d.fromSide].filter((p) => p.id !== source.id);
				save(next);
				return;
			}
			if (!d.moved && source) return;
			const size = source?.size ?? d.size;
			// Dropped with its pointer over its middle: place it so.
			const span = size === "large" ? 4 : size === "wide" ? 2 : 1;
			const cell = Math.max(0, t.side === "right" ? t.cell - Math.floor((span - 1) / 2) : t.cell - Math.floor((span - 1) / 2));
			const others = source && d.fromSide === t.side ? next[t.side].filter((p) => p.id !== source.id) : next[t.side];
			if (!fits(others, cell, size, grid.cells[t.side])) return;
			if (source && d.fromSide) next[d.fromSide] = next[d.fromSide].filter((p) => p.id !== source.id);
			const item: Placed = source
				? { ...source, cell }
				: { id: newId(), cell, size, widgets: [d.kind!], rotate: 0 };
			next[t.side] = [...next[t.side], item];
			save(next);
		};
		window.addEventListener("pointermove", move);
		window.addEventListener("pointerup", up, { once: true });
		return () => {
			window.removeEventListener("pointermove", move);
			window.removeEventListener("pointerup", up);
		};
	}, [drag, grid, layout]);

	// ---- changes from the menu
	const change = (side: Side, id: string, f: (p: Placed) => Placed | null) => {
		const next: Layout = { version: 1, left: layout.left, right: layout.right };
		next[side] = layout[side].map((p) => (p.id === id ? f(p) : p)).filter((p): p is Placed => p !== null);
		save(next);
	};
	const setSize = (side: Side, item: Placed, size: WidgetSize) => {
		if (!grid || !fits(layout[side], item.cell, size, grid.cells[side], item.id)) return false;
		change(side, item.id, (p) => ({ ...p, size }));
		return true;
	};
	const takeOut = (side: Side, item: Placed, kind: WidgetKind) => {
		if (!grid) return;
		const rest = item.widgets.filter((k) => k !== kind);
		const others = layout[side].map((p) => (p.id === item.id ? { ...p, widgets: rest } : p));
		const size = widgetInfo(kind)?.size ?? "small";
		let cell = 0;
		while (cell < grid.cells[side] && !fits(others, cell, size, grid.cells[side])) cell += 1;
		if (cell >= grid.cells[side]) return;
		save({ ...layout, [side]: [...others, { id: newId(), cell, size, widgets: [kind], rotate: 0 }] });
	};
	const setOption = (side: Side, item: Placed, kind: WidgetKind, key: string, value: string) =>
		change(side, item.id, (p) => ({
			...p,
			options: { ...(p.options ?? {}), [kind]: { ...(p.options?.[kind] ?? {}), [key]: value } }
		}));

	if (!grid || !dock) return null;
	const bottomBand = dock.height;
	const menuItem = menu ? layout[menu.side].find((p) => p.id === menu.itemId) : undefined;
	const menuKind = menuItem ? menuItem.widgets[(shown[menuItem.id] ?? 0) % menuItem.widgets.length] : undefined;

	return (
		<div className={`dw-root${editing ? " is-editing" : ""}`}>
			{/* The grid, while editing */}
			{editing &&
				(["left", "right"] as Side[]).map((side) =>
					Array.from({ length: grid.cells[side] }, (_, cell) => {
						const place = placeOf(grid, side, { cell, size: "small" });
						return (
							<div
								key={`${side}${cell}`}
								className="dw-cell"
								style={{ left: place.left, width: grid.cell, height: grid.cell, bottom: grid.pad }}
							/>
						);
					})
				)}

			{shelfRects.map((shelf) => (
				<div
					key={shelf.items.map((p) => p.id).join("-")}
					className={`dw-shelf${sideHidden(shelf.side) ? " is-hidden" : ""}`}
					style={{ left: shelf.left, width: shelf.width, height: bottomBand, borderRadius: `${18 * scale}px ${18 * scale}px 0 0` }}
				>
					<EdgeSurface edge="bottom" flares />
					{shelf.items.map((item) => {
						const place = placeOf(grid, shelf.side, item);
						const index = (shown[item.id] ?? 0) % item.widgets.length;
						const kind = item.widgets[index];
						const dragging = drag?.itemId === item.id && drag.moved;
						return (
							<div
								key={item.id}
								className={`dw-item dw-item--${item.size}${dragging ? " is-dragging" : ""}`}
								style={{ left: place.left - shelf.left, width: place.width, height: grid.cell, bottom: grid.pad }}
								onPointerEnter={() => (hovered.current = item.id)}
								onPointerLeave={() => (hovered.current = null)}
								onWheel={(e) => {
									if (item.widgets.length > 1) flip(item, e.deltaY > 0 ? 1 : -1);
								}}
								onPointerDown={(e) => {
									if (!editing || e.button !== 0) return;
									e.preventDefault();
									setDrag({ itemId: item.id, fromSide: shelf.side, size: item.size, x: e.clientX, y: e.clientY, moved: false });
								}}
								onContextMenu={(e) => {
									e.preventDefault();
									e.stopPropagation();
									setMenu({ side: shelf.side, itemId: item.id, x: e.clientX, y: e.clientY });
								}}
							>
								<div className="dw-face" key={kind}>
									<Widget kind={kind} size={item.size} id={item.id} options={item.options?.[kind]} data={data} />
								</div>
								{item.widgets.length > 1 && (
									<div className="dw-dots">
										{item.widgets.map((k, i) => (
											<span key={k} className={i === index ? "is-on" : ""} />
										))}
									</div>
								)}
								{editing && (
									<button
										type="button"
										className="dw-remove"
										title="Remove"
										onPointerDown={(e) => e.stopPropagation()}
										onClick={(e) => {
											e.stopPropagation();
											change(shelf.side, item.id, () => null);
										}}
									>
										<X size={10} strokeWidth={2.4} />
									</button>
								)}
							</div>
						);
					})}
				</div>
			))}

			{/* Where a dragged widget would land */}
			{drag?.moved &&
				(() => {
					const t = target(drag.x, drag.y);
					if (!t) return null;
					if (t.over && t.over.id !== drag.itemId) {
						const place = placeOf(grid, t.side, t.over);
						return <div className="dw-drop dw-drop--group" style={{ left: place.left, width: place.width, height: grid.cell, bottom: grid.pad }} />;
					}
					const span = drag.size === "large" ? 4 : drag.size === "wide" ? 2 : 1;
					const cell = Math.max(0, t.cell - Math.floor((span - 1) / 2));
					const place = placeOf(grid, t.side, { cell, size: drag.size });
					const others = drag.itemId && drag.fromSide === t.side ? layout[t.side].filter((p) => p.id !== drag.itemId) : layout[t.side];
					const ok = fits(others, cell, drag.size, grid.cells[t.side]);
					return (
						<div
							className={`dw-drop${ok ? "" : " is-blocked"}`}
							style={{ left: place.left, width: place.width, height: grid.cell, bottom: grid.pad }}
						/>
					);
				})()}

			{/* The picker */}
			{editing && (
				<div ref={pickerRef} className="dw-picker" style={{ bottom: bottomBand + 18 * scale }}>
					<div className="dw-picker-head">
						<Layers size={14} />
						<span>Drag widgets beside the dock. Drop one on another to group them.</span>
						<button type="button" className="dw-done" onClick={() => setEditing(false)}>
							<Check size={13} /> Done
						</button>
					</div>
					<div className="dw-picker-sections">
						{(["Status", "Clock & calendar", "Now playing", "Tasks & timers"] as const).map((section) => (
							<div key={section} className="dw-picker-section">
								<span className="dw-picker-title">{section}</span>
								<div className="dw-picker-list">
									{WIDGETS.filter((w) => w.section === section).map((w) => (
										<button
											type="button"
											key={w.kind}
											className="dw-chip"
											onPointerDown={(e) => {
												e.preventDefault();
												setDrag({ kind: w.kind, size: w.size, x: e.clientX, y: e.clientY, moved: false });
											}}
										>
											<Plus size={11} /> {w.label}
										</button>
									))}
								</div>
							</div>
						))}
					</div>
				</div>
			)}

			{/* A chip following the pointer while dragging from the picker */}
			{drag?.moved && drag.kind && (
				<div className="dw-ghost" style={{ left: drag.x + 10, top: drag.y + 10 }}>
					{widgetInfo(drag.kind)?.label}
				</div>
			)}

			{/* A widget's menu */}
			{menu && menuItem && menuKind && (
				<>
					<div className="dw-menu-backdrop" onPointerDown={() => setMenu(null)} />
					<div
						ref={menuRef}
						className="dw-menu"
						style={{ left: Math.min(menu.x, window.innerWidth - 240), bottom: window.innerHeight - menu.y + 6 }}
						onPointerDown={(e) => e.stopPropagation()}
					>
						<div className="dw-menu-title">{widgetInfo(menuKind)?.label}</div>
						<div className="dw-menu-row">
							{(["small", "wide", "large"] as WidgetSize[]).map((size) => (
								<button
									type="button"
									key={size}
									className={`dw-seg${menuItem.size === size ? " is-on" : ""}`}
									disabled={!fits(layout[menu.side], menuItem.cell, size, grid.cells[menu.side], menuItem.id)}
									onClick={() => setSize(menu.side, menuItem, size)}
								>
									{size === "small" ? "Small" : size === "wide" ? "Wide" : "Large"}
								</button>
							))}
						</div>
						{menuKind === "worldclock" && (
							<select
								className="dw-select"
								value={menuItem.options?.worldclock?.zone ?? "America/New_York"}
								onChange={(e) => setOption(menu.side, menuItem, "worldclock", "zone", e.target.value)}
							>
								{ZONES.map((z) => (
									<option key={z.zone} value={z.zone}>
										{z.city}
									</option>
								))}
							</select>
						)}
						{menuKind === "countdown" && (
							<div className="dw-menu-form">
								<input
									className="dw-input"
									placeholder="What's coming"
									defaultValue={menuItem.options?.countdown?.label ?? ""}
									onBlur={(e) => setOption(menu.side, menuItem, "countdown", "label", e.target.value)}
								/>
								<input
									className="dw-input"
									type="date"
									defaultValue={menuItem.options?.countdown?.date ?? ""}
									onChange={(e) => setOption(menu.side, menuItem, "countdown", "date", e.target.value)}
								/>
							</div>
						)}
						{menuKind === "focus" && (
							<div className="dw-menu-row">
								{FOCUS_LENGTHS.map((min) => (
									<button
										type="button"
										key={min}
										className={`dw-seg${readFocus(menuItem.id).length === min ? " is-on" : ""}`}
										onClick={() => writeFocus(menuItem.id, { length: min, endsAt: null, left: min * 60 })}
									>
										{min} min
									</button>
								))}
							</div>
						)}
						{menuItem.widgets.length > 1 && (
							<>
								<div className="dw-menu-label">Turn on its own</div>
								<div className="dw-menu-row">
									{ROTATIONS.map((sec) => (
										<button
											type="button"
											key={sec}
											className={`dw-seg${menuItem.rotate === sec ? " is-on" : ""}`}
											onClick={() => change(menu.side, menuItem.id, (p) => ({ ...p, rotate: sec }))}
										>
											{sec === 0 ? "Off" : `${sec} s`}
										</button>
									))}
								</div>
								<button type="button" className="dw-menu-item" onClick={() => (takeOut(menu.side, menuItem, menuKind), setMenu(null))}>
									Take {widgetInfo(menuKind)?.label} out of the group
								</button>
							</>
						)}
						<button type="button" className="dw-menu-item" onClick={() => (setEditing(true), setMenu(null))}>
							Edit widgets
						</button>
						<button
							type="button"
							className="dw-menu-item dw-danger"
							onClick={() => {
								if (menuItem.widgets.length > 1) {
									const rest = menuItem.widgets.filter((k) => k !== menuKind);
									change(menu.side, menuItem.id, (p) => ({ ...p, widgets: rest }));
								} else change(menu.side, menuItem.id, () => null);
								setMenu(null);
							}}
						>
							Remove {widgetInfo(menuKind)?.label}
						</button>
					</div>
				</>
			)}
		</div>
	);
}

