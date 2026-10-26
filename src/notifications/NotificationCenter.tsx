import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { Bell, BellOff, ChevronDown, PanelRight, ShieldAlert, X } from "lucide-react";
import { initTheme } from "../theme";
import { useSettingsSync } from "../hooks/useSettingsSync";
import { ToastActions, type ToastButton, type ToastInput } from "./ToastActions";
import "./Notifications.css";
import "./NotificationCenter.css";

/**
 * Bloom's notification center (Settings > Notifications > Notification
 * Center: Bloom): the same notifications as Windows' sidebar, in Bloom's look,
 * with their buttons and reply boxes, plus every notification Bloom showed as
 * a card until it is clicked or cleared, grouped by app (the app with the
 * newest first). An app's group shows only its newest notification, with the
 * others stacked behind it; opened (its arrow, or the stack), it lists them
 * all, newest at the top. It hangs
 * from the top of the screen and is only as tall as its notifications; past
 * about six the list scrolls.
 * Opened by the notch's bell button; closes when it loses focus or on Escape.
 */

interface Item {
	id: number;
	app: string;
	aumid: string;
	icon: string | null;
	title: string;
	body: string;
	time: number;
	buttons: ToastButton[];
	inputs: ToastInput[];
}

const appWindow = getCurrentWebviewWindow();

/** An app's group shows its newest notification until it is opened. */
const GROUP_PREVIEW = 1;

interface Group {
	key: string;
	app: string;
	icon: string | null;
	items: Item[];
}

/** One group per app, the app with the newest notification first. */
function groupByApp(items: Item[]): Group[] {
	const groups = new Map<string, Group>();
	for (const item of items) {
		const key = (item.aumid || item.app || "notification").toLowerCase();
		let group = groups.get(key);
		if (!group) {
			group = { key, app: item.app, icon: item.icon, items: [] };
			groups.set(key, group);
		}
		group.items.push(item);
		if (!group.icon && item.icon) group.icon = item.icon;
		if (!group.app && item.app) group.app = item.app;
	}
	const list = [...groups.values()];
	for (const group of list) group.items.sort((a, b) => b.time - a.time);
	list.sort((a, b) => b.items[0].time - a.items[0].time);
	return list;
}

function formatTime(ms: number) {
	const date = new Date(ms);
	const now = new Date();
	const minutes = Math.floor((now.getTime() - ms) / 60000);
	if (minutes < 1) return "now";
	if (minutes < 60) return `${minutes} min ago`;
	const pad = (n: number) => String(n).padStart(2, "0");
	const h24 = localStorage.getItem("bloom-time-format-24h") === "true";
	const clock = h24
		? `${pad(date.getHours())}:${pad(date.getMinutes())}`
		: date.toLocaleTimeString([], { hour: "numeric", minute: "2-digit", hour12: true });
	if (date.toDateString() === now.toDateString()) return clock;
	return `${pad(date.getDate())}-${pad(date.getMonth() + 1)} ${clock}`;
}

export default function NotificationCenter() {
	const [items, setItems] = useState<Item[]>([]);
	const [problem, setProblem] = useState<string | null>(null);
	const [radius, setRadius] = useState(() =>
		Number(localStorage.getItem("bloom-notify-radius") ?? 16)
	);
	const panelRef = useRef<HTMLDivElement>(null);

	// The window follows the panel's height, so nothing below it blocks clicks.
	useEffect(() => {
		const panel = panelRef.current;
		if (!panel) return;
		const fit = () => {
			const height = Math.ceil(panel.getBoundingClientRect().height);
			if (height > 0) void invoke("notification_center_fit", { height }).catch(() => {});
		};
		const observer = new ResizeObserver(fit);
		observer.observe(panel);
		fit();
		return () => observer.disconnect();
	}, []);

	useEffect(() => initTheme(), []);
	useSettingsSync({ "bloom-notify-radius": (v) => setRadius(Number(v) || 0) });

	const refresh = useCallback(async () => {
		try {
			setItems(await invoke<Item[]>("notification_center_list"));
			setProblem(null);
		} catch (e) {
			setProblem(String(e));
		}
	}, []);

	useEffect(() => {
		void refresh();
		// Read when opened, and while open when a notification arrives, is
		// clicked or is cleared; nothing runs while the center is closed.
		const opened = listen("notification-center-open", () => void refresh());
		const changed = listen("notification-center-changed", () => {
			void appWindow.isVisible().then((visible) => {
				if (visible) void refresh();
			});
		});
		const blur = appWindow.onFocusChanged(({ payload: focused }) => {
			if (!focused) void invoke("notification_center_hide");
		});
		const onKey = (e: KeyboardEvent) => {
			if (e.key === "Escape") void invoke("notification_center_hide");
		};
		window.addEventListener("keydown", onKey);
		return () => {
			void opened.then((f) => f());
			void changed.then((f) => f());
			void blur.then((f) => f());
			window.removeEventListener("keydown", onKey);
		};
	}, [refresh]);

	const remove = (id: number) => {
		setItems((prev) => prev.filter((n) => n.id !== id));
		void invoke("notification_remove", { id }).catch(console.error);
	};

	const groups = useMemo(() => groupByApp(items), [items]);
	const [opened, setOpened] = useState<Set<string>>(() => new Set());
	const toggleGroup = (key: string) =>
		setOpened((prev) => {
			const next = new Set(prev);
			if (next.has(key)) next.delete(key);
			else next.add(key);
			return next;
		});
	const clearGroup = (group: Group) => {
		const ids = new Set(group.items.map((item) => item.id));
		setItems((prev) => prev.filter((n) => !ids.has(n.id)));
		for (const id of ids) void invoke("notification_remove", { id }).catch(console.error);
	};

	const openWindowsCenter = () => {
		void invoke("notification_center_hide");
		void invoke("open_windows_notification_center");
	};

	return (
		<div
			ref={panelRef}
			className="center-panel"
			style={
				{
					"--notify-radius": `${radius}px`,
					// Room for about six notifications; more scroll.
					"--center-list-max": `${Math.max(240, Math.min(600, window.screen.availHeight - 140))}px`
				} as React.CSSProperties
			}
		>
			<div className="center-header">
				<span className="center-title">Notifications</span>
				<div className="center-header-actions">
					{items.length > 0 && (
						<button
							type="button"
							className="center-clear"
							onClick={() => {
								setItems([]);
								void invoke("notification_clear_all").catch(console.error);
							}}
						>
							Clear all
						</button>
					)}
					<button
						type="button"
						className="center-icon-button"
						title="Windows' notification center (Do not disturb, notification settings)"
						onClick={openWindowsCenter}
					>
						<PanelRight size={14} />
					</button>
				</div>
			</div>

			{problem ? (
				<div className="center-empty">
					<ShieldAlert size={22} />
					<span>
						{problem === "denied"
							? "Windows doesn't let Bloom read notifications yet."
							: "Windows won't share notifications with Bloom."}
					</span>
					<div className="center-actions">
						{problem === "denied" && (
							<button type="button" onClick={() => void invoke("notifications_open_privacy")}>
								Allow access
							</button>
						)}
						<button type="button" onClick={openWindowsCenter}>
							Open Windows' center
						</button>
					</div>
				</div>
			) : items.length === 0 ? (
				<div className="center-empty">
					<BellOff size={22} />
					<span>No new notifications</span>
				</div>
			) : (
				<div className="center-list">
					<AnimatePresence initial={false}>
						{groups.map((group) => {
							const open = opened.has(group.key);
							const hiddenCount = open ? 0 : Math.max(0, group.items.length - GROUP_PREVIEW);
							// The cards stacked behind the newest one (at most two drawn).
							const stacked = Math.min(hiddenCount, 2);
							const shown = open ? group.items : group.items.slice(0, GROUP_PREVIEW);
							return (
								<motion.div
									key={group.key}
									layout
									className="center-group"
									initial={{ opacity: 0, y: -8 }}
									animate={{ opacity: 1, y: 0 }}
									exit={{ opacity: 0, x: 40, transition: { duration: 0.15 } }}
								>
									<div className="center-group-head">
										{group.icon ? (
											<img className="notify-app-icon" src={group.icon} alt="" draggable={false} />
										) : (
											<Bell className="notify-app-icon" size={14} />
										)}
										<span className="center-group-name">{group.app || "Notification"}</span>
										{group.items.length > 1 && (
											<span className="center-group-count">{group.items.length}</span>
										)}
										{group.items.length > GROUP_PREVIEW && (
											<button
												type="button"
												className="center-group-button"
												title={open ? "Collapse" : `Show all ${group.items.length}`}
												aria-expanded={open}
												onClick={() => toggleGroup(group.key)}
											>
												<ChevronDown
													size={14}
													style={{
														transform: open ? "rotate(180deg)" : undefined,
														transition: "transform 0.15s ease"
													}}
												/>
											</button>
										)}
										<button
											type="button"
											className="center-group-button"
											title={`Clear ${group.app || "these"}`}
											onClick={() => clearGroup(group)}
										>
											<X size={13} />
										</button>
									</div>
									<div
										className={`center-stack${stacked ? ` depth-${stacked}` : ""}`}
									>
									<AnimatePresence initial={false}>
										{shown.map((item) => (
											<motion.div
												key={item.id}
												layout
												className="notify-card center-card"
												initial={{ opacity: 0, y: -8 }}
												animate={{ opacity: 1, y: 0 }}
												exit={{ opacity: 0, x: 40, transition: { duration: 0.15 } }}
												onClick={() => {
													setItems((prev) => prev.filter((n) => n.id !== item.id));
													void invoke("notification_activate", {
														id: item.id,
														aumid: item.aumid,
														time: item.time
													}).catch(console.error);
													void invoke("notification_center_hide");
												}}
											>
												<div className="notify-head">
													<span className="notify-title center-card-title">{item.title}</span>
													<span className="notify-time">{formatTime(item.time)}</span>
													<button
														type="button"
														className="notify-close"
														title="Dismiss"
														onClick={(e) => {
															e.stopPropagation();
															remove(item.id);
														}}
													>
														<X size={12} />
													</button>
												</div>
												{item.body && <div className="notify-body">{item.body}</div>}
												<ToastActions
													buttons={item.buttons ?? []}
													inputs={item.inputs ?? []}
													onPress={(button, typed) => {
														setItems((prev) => prev.filter((n) => n.id !== item.id));
														invoke("notification_action", {
															id: item.id,
															aumid: item.aumid,
															time: item.time,
															button,
															typed
														}).catch(console.error);
													}}
												/>
											</motion.div>
										))}
									</AnimatePresence>
									{stacked > 0 && (
										<button
											type="button"
											className="center-stack-layers"
											aria-label={`Show all ${group.items.length} from ${group.app || "this app"}`}
											title={`${hiddenCount} more`}
											onClick={() => toggleGroup(group.key)}
										>
											{Array.from({ length: stacked }, (_, i) => (
												<span key={i} className={`center-stack-layer layer-${i + 1}`} />
											))}
										</button>
									)}
									</div>
								</motion.div>
							);
						})}
					</AnimatePresence>
				</div>
			)}
		</div>
	);
}
