import { useEffect, useRef, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Bell, X } from "lucide-react";
import { initTheme } from "../theme";
import { useSettingsSync } from "../hooks/useSettingsSync";
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
	const stackRef = useRef<HTMLDivElement>(null);
	const nextKey = useRef(1);
	const timers = useRef(new Map<number, number>());

	useEffect(() => initTheme(), []);

	useSettingsSync({
		"bloom-notify-radius": (v) => setRadius(Number(v) || 0),
		"bloom-notify-duration": (v) => setDuration(Number(v) || 6),
		"bloom-notify-position": (v) => setPosition(String(v))
	});

	const remove = (key: number) => {
		const timer = timers.current.get(key);
		if (timer !== undefined) window.clearTimeout(timer);
		timers.current.delete(key);
		setCards((prev) => prev.filter((c) => c.key !== key));
	};

	useEffect(() => {
		const unlisten = listen<Incoming>("bloom-notification", (event) => {
			const card = { ...event.payload, key: nextKey.current++ };
			setCards((prev) => [card, ...prev].slice(0, MAX_CARDS));
		});
		return () => {
			void unlisten.then((fn) => fn());
		};
	}, []);

	// Each card leaves after the set time; hovering the stack holds them all.
	useEffect(() => {
		if (hovered) {
			for (const timer of timers.current.values()) window.clearTimeout(timer);
			timers.current.clear();
			return;
		}
		for (const card of cards) {
			if (timers.current.has(card.key)) continue;
			timers.current.set(
				card.key,
				window.setTimeout(() => remove(card.key), Math.max(2, duration) * 1000)
			);
		}
	}, [cards, hovered, duration]);

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
							remove(card.key);
							if (card.id !== 0) {
								invoke("notification_activate", { id: card.id, aumid: card.aumid }).catch(
									console.error
								);
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
					</motion.div>
				))}
			</AnimatePresence>
		</div>
	);
}
