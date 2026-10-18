import { useCallback, useEffect, useRef, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { Bell, BellOff, PanelRight, ShieldAlert, X } from "lucide-react";
import { initTheme } from "../theme";
import { useSettingsSync } from "../hooks/useSettingsSync";
import { ToastActions, type ToastButton, type ToastInput } from "./ToastActions";
import "./Notifications.css";
import "./NotificationCenter.css";

/**
 * Bloom's notification center (Settings > Notifications > Notification
 * Center: Bloom): the same notifications as Windows' sidebar, in Bloom's look,
 * with their buttons and reply boxes, plus every notification Bloom showed as
 * a card until it is clicked or cleared. It hangs from the top of the screen and
 * is only as tall as its notifications; past about six the list scrolls.
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
						{items.map((item) => (
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
									{item.icon ? (
										<img className="notify-app-icon" src={item.icon} alt="" draggable={false} />
									) : (
										<Bell className="notify-app-icon" size={14} />
									)}
									<span className="notify-app">{item.app || "Notification"}</span>
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
								<div className="notify-title">{item.title}</div>
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
				</div>
			)}
		</div>
	);
}
