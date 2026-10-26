import { useEffect, useRef, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Bell, X } from "lucide-react";
import { initTheme } from "../theme";
import { useSettingsSync } from "../hooks/useSettingsSync";
import { ToastActions, type ToastButton, type ToastInput } from "./ToastActions";
import "./Notifications.css";

/**
 * Bloom-style notification cards (Settings > Notifications > Style: Bloom).
 * The backend reads Windows' notifications and sends each new one here; this
 * window is sized to the cards and sits in the chosen corner.
 */

interface Incoming {
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

interface Card extends Incoming {
	key: number;
}

/** Card width plus room for its shadow. */
const WINDOW_WIDTH = 384;
const MAX_CARDS = 4;

function readNumber(key: string, fallback: number) {
	const value = parseFloat(localStorage.getItem(key) ?? "");
	return Number.isFinite(value) ? value : fallback;
}

export default function Notifications() {
	const [cards, setCards] = useState<Card[]>([]);
	const [radius, setRadius] = useState(() => readNumber("bloom-notify-radius", 16));
	const [duration, setDuration] = useState(() => readNumber("bloom-notify-duration", 6));
	const [position, setPosition] = useState(
		() => localStorage.getItem("bloom-notify-position") ?? "default"
	);
	const [hovered, setHovered] = useState(false);
	const [typing, setTyping] = useState(false);
	const stackRef = useRef<HTMLDivElement>(null);
	const nextKey = useRef(1);
	const timers = useRef(new Map<number, number>());

	useEffect(() => initTheme(), []);

	useSettingsSync({
		"bloom-notify-radius": (v) => setRadius(Number(v) || 0),
		"bloom-notify-duration": (v) => setDuration(Number(v) || 6),
		"bloom-notify-position": (v) => setPosition(String(v))
	});

	// Windows' own pop-up for a notification stays (hidden) while its card
	// shows, so a click on the card can still open what it is about; a card
	// that closes without being clicked closes that pop-up too.
	const closeWindowsPopup = (card: { id: number; title: string; body?: string }) => {
		if (card.id === 0) return;
		void invoke("notification_card_closed", {
			title: card.title ?? "",
			body: card.body ?? ""
		}).catch(() => {});
	};

	const remove = (key: number, closeInWindows = true) => {
		const timer = timers.current.get(key);
		if (timer !== undefined) window.clearTimeout(timer);
		timers.current.delete(key);
		setCards((prev) => {
			const gone = prev.find((c) => c.key === key);
			if (gone && closeInWindows) closeWindowsPopup(gone);
			return prev.filter((c) => c.key !== key);
		});
	};

	useEffect(() => {
		const add = (incoming: Incoming) => {
			const card = { ...incoming, key: nextKey.current++ };
			setCards((prev) => {
				const next = [card, ...prev];
				// Pushed out by newer cards: closed like any other.
				// Pushed out by newer cards: their pop-ups stay, as for time up.
				return next.slice(0, MAX_CARDS);
			});
		};
		const unlisten = listen<Incoming>("bloom-notification", (event) => add(event.payload));
		// Listening now: the cards that came before this page was ready
		// (this window is made a moment after Bloom starts).
		void unlisten
			.then(() => invoke<Incoming[]>("notifications_ready"))
			.then((waiting) => waiting.forEach(add))
			.catch(() => {});
		return () => {
			void unlisten.then((fn) => fn());
		};
	}, []);

	// Each card leaves after the set time; hovering the stack, or typing a
	// reply, holds them all.
	useEffect(() => {
		if (hovered || typing) {
			for (const timer of timers.current.values()) window.clearTimeout(timer);
			timers.current.clear();
			return;
		}
		for (const card of cards) {
			if (timers.current.has(card.key)) continue;
			timers.current.set(
				card.key,
				// Time up: only the card goes. Windows' hidden pop-up stays, so a
				// click from Bloom's center later still reaches the app (apps such
				// as Discord forget a notification once its pop-up closes).
				window.setTimeout(() => remove(card.key, false), Math.max(2, duration) * 1000)
			);
		}
	}, [cards, hovered, typing, duration]);

	const allowTyping = (on: boolean) => {
		setTyping(on);
		void invoke("notifications_allow_typing", { typing: on }).catch(() => {});
	};

	// The window follows the stack's height; an empty stack hides it.
	useEffect(() => {
		const el = stackRef.current;
		if (!el) return;
		const resize = () => {
			const height = cards.length === 0 ? 0 : Math.ceil(el.getBoundingClientRect().height);
			void invoke("notifications_set_size", { width: WINDOW_WIDTH, height });
		};
		const observer = new ResizeObserver(resize);
		observer.observe(el);
		resize();
		return () => observer.disconnect();
	}, [cards.length, position]);

	const fromTop = position === "top-right" || position === "top-left";
	const fromLeft = position === "top-left" || position === "bottom-left";

	return (
		<div
			ref={stackRef}
			className={`notify-stack ${fromTop ? "from-top" : "from-bottom"}`}
			style={{ "--notify-radius": `${radius}px` } as React.CSSProperties}
			onMouseEnter={() => setHovered(true)}
			onMouseLeave={() => setHovered(false)}
		>
			<AnimatePresence initial={false}>
				{cards.map((card) => (
					<motion.div
						key={card.key}
						layout
						className="notify-card"
						initial={{ opacity: 0, x: fromLeft ? -40 : 40 }}
						animate={{ opacity: 1, x: 0 }}
						exit={{ opacity: 0, x: fromLeft ? -40 : 40, transition: { duration: 0.15 } }}
						transition={{ type: "spring", stiffness: 420, damping: 34 }}
						onClick={() => {
							// Bloom clicks Windows' pop-up for it, which closes it.
							remove(card.key, false);
							if (card.id !== 0) {
								invoke("notification_activate", {
									id: card.id,
									aumid: card.aumid,
									time: card.time
								}).catch(console.error);
							}
						}}
					>
						<div className="notify-head">
							{card.icon ? (
								<img className="notify-app-icon" src={card.icon} alt="" draggable={false} />
							) : (
								<Bell className="notify-app-icon" size={14} />
							)}
							<span className="notify-app">{card.app || "Notification"}</span>
							<span className="notify-time">now</span>
							<button
								type="button"
								className="notify-close"
								title="Dismiss"
								onClick={(e) => {
									e.stopPropagation();
									remove(card.key);
								}}
							>
								<X size={12} />
							</button>
						</div>
						<div className="notify-title">{card.title}</div>
						{card.body && <div className="notify-body">{card.body}</div>}
						<ToastActions
							buttons={card.buttons ?? []}
							inputs={card.inputs ?? []}
							onTyping={allowTyping}
							onPress={(button, typed) => {
								remove(card.key, false);
								allowTyping(false);
								invoke("notification_action", {
									id: card.id,
									aumid: card.aumid,
									time: card.time,
									button,
									typed
								}).catch(console.error);
							}}
						/>
					</motion.div>
				))}
			</AnimatePresence>
		</div>
	);
}
