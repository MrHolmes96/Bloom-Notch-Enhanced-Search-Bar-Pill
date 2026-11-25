import { memo, useEffect, useState, type ComponentType } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
	ArrowDown,
	ArrowUp,
	BatteryCharging,
	BatteryFull,
	BatteryLow,
	BatteryMedium,
	Calendar,
	CheckCircle2,
	Cpu,
	Globe2,
	HardDrive,
	Hourglass,
	MemoryStick,
	Music2,
	Pause,
	Play,
	SkipForward,
	Timer,
	TimerReset
} from "lucide-react";
import { formatClock } from "../hooks/useClock24h";
import { speed, type Media, type Metrics, type TodayTasks } from "./data";
import type { WidgetKind, WidgetSize } from "./model";

/** Everything the widgets read, gathered once by DockWidgets. */
export interface WidgetData {
	metrics: Metrics;
	weather: { temperature: number | null; condition: string; icon: ComponentType<any>; city: string; unit: string };
	media: Media | null;
	today: TodayTasks | null;
	now: Date;
	h24: boolean;
}

type Opts = Record<string, string> | undefined;

/** Time zones offered for the world clock (Windows' own names would need a lookup). */
export const ZONES: { zone: string; city: string }[] = [
	{ zone: "America/Los_Angeles", city: "Los Angeles" },
	{ zone: "America/New_York", city: "New York" },
	{ zone: "America/Sao_Paulo", city: "São Paulo" },
	{ zone: "Europe/London", city: "London" },
	{ zone: "Europe/Copenhagen", city: "Copenhagen" },
	{ zone: "Europe/Berlin", city: "Berlin" },
	{ zone: "Europe/Istanbul", city: "Istanbul" },
	{ zone: "Asia/Dubai", city: "Dubai" },
	{ zone: "Asia/Kolkata", city: "Mumbai" },
	{ zone: "Asia/Singapore", city: "Singapore" },
	{ zone: "Asia/Shanghai", city: "Shanghai" },
	{ zone: "Asia/Tokyo", city: "Tokyo" },
	{ zone: "Australia/Sydney", city: "Sydney" },
	{ zone: "Pacific/Auckland", city: "Auckland" }
];

function Stat({
	icon: Icon,
	label,
	value,
	size,
	fill,
	color
}: {
	icon: ComponentType<any>;
	label: string;
	value: string;
	size: WidgetSize;
	fill?: number | null;
	color?: string;
}) {
	if (size === "small")
		return (
			<div className="dw-stat dw-stat--small" title={`${label} ${value}`}>
				<Icon size={14} strokeWidth={1.8} style={{ color }} />
				<span className="dw-value">{value}</span>
			</div>
		);
	return (
		<div className="dw-stat">
			<Icon size={16} strokeWidth={1.8} style={{ color }} />
			<div className="dw-stat-text">
				<span className="dw-label">{label}</span>
				<span className="dw-value">{value}</span>
				{fill != null && size === "large" && (
					<span className="dw-meter">
						<span style={{ width: `${Math.max(0, Math.min(100, fill))}%`, background: color }} />
					</span>
				)}
			</div>
		</div>
	);
}

const pct = (n: number | null) => (n === null ? "–" : `${Math.round(n)}%`);

function batteryIcon(level: number | null, charging: boolean) {
	if (charging) return BatteryCharging;
	if (level === null || level > 70) return BatteryFull;
	if (level > 30) return BatteryMedium;
	return BatteryLow;
}

// ---------------------------------------------------------------- focus timer

interface FocusState {
	/** Minutes per round. */
	length: number;
	/** When a running round ends (ms since 1970), or null when paused. */
	endsAt: number | null;
	/** Seconds left while paused. */
	left: number;
}

const focusKey = (id: string) => `bloom-dock-focus-${id}`;

export function readFocus(id: string): FocusState {
	try {
		const raw = JSON.parse(localStorage.getItem(focusKey(id)) ?? "null");
		if (raw && typeof raw.length === "number")
			return { length: raw.length, endsAt: raw.endsAt ?? null, left: raw.left ?? raw.length * 60 };
	} catch {}
	return { length: 25, endsAt: null, left: 25 * 60 };
}

export function writeFocus(id: string, state: FocusState) {
	try {
		localStorage.setItem(focusKey(id), JSON.stringify(state));
	} catch {}
	window.dispatchEvent(new CustomEvent("dw-focus", { detail: id }));
}

function Focus({ id, size, now }: { id: string; size: WidgetSize; now: Date }) {
	const [state, setState] = useState(() => readFocus(id));
	useEffect(() => {
		const changed = (e: Event) => {
			if ((e as CustomEvent).detail === id) setState(readFocus(id));
		};
		window.addEventListener("dw-focus", changed);
		return () => window.removeEventListener("dw-focus", changed);
	}, [id]);
	const left = state.endsAt ? Math.max(0, Math.round((state.endsAt - now.getTime()) / 1000)) : state.left;
	const done = state.endsAt !== null && left === 0;
	const running = state.endsAt !== null && !done;
	const text = `${Math.floor(left / 60)}:${String(left % 60).padStart(2, "0")}`;
	const toggle = (e: React.MouseEvent) => {
		e.stopPropagation();
		if (done) writeFocus(id, { ...state, endsAt: null, left: state.length * 60 });
		else if (running) writeFocus(id, { ...state, endsAt: null, left });
		else writeFocus(id, { ...state, endsAt: Date.now() + left * 1000 });
	};
	return (
		<button type="button" className={`dw-focus${done ? " is-done" : ""}`} onClick={toggle} title="Focus timer">
			{done ? <TimerReset size={15} /> : running ? <Pause size={15} /> : <Play size={15} />}
			{size !== "small" && <span className="dw-value dw-big">{done ? "Done" : text}</span>}
			{size === "large" && <span className="dw-label">{running ? "Focusing" : done ? "Round over" : "Focus"}</span>}
		</button>
	);
}

// ---------------------------------------------------------------- the widgets

export function Widget({
	kind,
	size,
	id,
	options,
	data
}: {
	kind: WidgetKind;
	size: WidgetSize;
	id: string;
	options: Opts;
	data: WidgetData;
}) {
	const { metrics: m, weather: w, media, today, now, h24 } = data;
	switch (kind) {
		case "weather":
			return (
				<Stat
					icon={w.icon}
					label={size === "large" && w.city ? w.city : w.condition || "Weather"}
					value={w.temperature === null ? "–" : `${Math.round(w.temperature)}°${size === "small" ? "" : w.unit}`}
					size={size}
					color="#60a5fa"
				/>
			);
		case "battery":
			return (
				<Stat
					icon={batteryIcon(m.battery, m.charging)}
					label={m.charging ? "Charging" : "Battery"}
					value={pct(m.battery)}
					size={size}
					fill={m.battery}
					color="#4ade80"
				/>
			);
		case "cpu":
			return <Stat icon={Cpu} label="CPU" value={pct(m.cpu)} size={size} fill={m.cpu} color="#f97316" />;
		case "ram":
			return <Stat icon={MemoryStick} label="RAM" value={pct(m.ram)} size={size} fill={m.ram} color="#a78bfa" />;
		case "disk":
			return <Stat icon={HardDrive} label="Disk" value={pct(m.disk)} size={size} fill={m.disk} color="#38bdf8" />;
		case "net":
			return size === "small" ? (
				<Stat icon={ArrowDown} label="Download" value={speed(m.down).replace("/s", "")} size="small" color="#2dd4bf" />
			) : (
				<div className="dw-net">
					<span>
						<ArrowDown size={12} style={{ color: "#2dd4bf" }} /> {speed(m.down)}
					</span>
					<span>
						<ArrowUp size={12} style={{ color: "#2dd4bf" }} /> {speed(m.up)}
					</span>
				</div>
			);
		case "clock":
			return (
				<div className="dw-clock">
					<span className="dw-big">{formatClock(now, h24)}</span>
					{size !== "small" && (
						<span className="dw-label">
							{now.toLocaleDateString([], { weekday: "short", day: "numeric", month: size === "large" ? "long" : "short" })}
						</span>
					)}
				</div>
			);
		case "date": {
			const week = isoWeek(now);
			return size === "small" ? (
				<div className="dw-date">
					<span className="dw-label">{now.toLocaleDateString([], { weekday: "short" })}</span>
					<span className="dw-big">{now.getDate()}</span>
				</div>
			) : (
				<div className="dw-stat">
					<Calendar size={16} strokeWidth={1.8} style={{ color: "var(--windows-accent, #60cdff)" }} />
					<div className="dw-stat-text">
						<span className="dw-value">
							{now.toLocaleDateString([], { weekday: size === "large" ? "long" : "short", day: "numeric", month: "short" })}
						</span>
						<span className="dw-label">Week {week}</span>
					</div>
				</div>
			);
		}
		case "worldclock": {
			const zone = options?.zone || "America/New_York";
			const city = ZONES.find((z) => z.zone === zone)?.city ?? zone.split("/").pop()?.replace(/_/g, " ");
			let time = "–";
			try {
				time = now.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: !h24, timeZone: zone });
			} catch {}
			return size === "small" ? (
				<div className="dw-date" title={city}>
					<span className="dw-label">{city?.slice(0, 3)}</span>
					<span className="dw-value">{time}</span>
				</div>
			) : (
				<div className="dw-stat">
					<Globe2 size={16} strokeWidth={1.8} style={{ color: "#60a5fa" }} />
					<div className="dw-stat-text">
						<span className="dw-label">{city}</span>
						<span className="dw-value">{time}</span>
					</div>
				</div>
			);
		}
		case "media": {
			const art = media?.artwork?.[0];
			const playing = !!media?.is_playing;
			const has = !!media?.has_media;
			const control = (e: React.MouseEvent, command: string) => {
				e.stopPropagation();
				void invoke(command).catch(() => {});
			};
			return (
				<div className="dw-media">
					<div className="dw-art" onClick={(e) => control(e, "media_play_pause")} title={playing ? "Pause" : "Play"}>
						{art ? <img src={art} alt="" /> : <Music2 size={16} />}
						{size === "small" && has && <span className="dw-art-state">{playing ? <Pause size={12} /> : <Play size={12} />}</span>}
					</div>
					{size !== "small" && (
						<>
							{size === "large" && (
								<div className="dw-stat-text dw-media-text">
									<span className="dw-value">{has ? media?.title || "Unknown" : "Nothing playing"}</span>
									<span className="dw-label">{has ? media?.artist : ""}</span>
								</div>
							)}
							<button type="button" className="dw-icon-button" onClick={(e) => control(e, "media_play_pause")} disabled={!has}>
								{playing ? <Pause size={15} /> : <Play size={15} />}
							</button>
							<button type="button" className="dw-icon-button" onClick={(e) => control(e, "media_next")} disabled={!has}>
								<SkipForward size={15} />
							</button>
						</>
					)}
				</div>
			);
		}
		case "tasks":
			return size === "small" ? (
				<Stat icon={CheckCircle2} label="Today" value={today ? String(today.open) : "–"} size="small" color="#4caf50" />
			) : (
				<div className="dw-stat">
					<CheckCircle2 size={16} strokeWidth={1.8} style={{ color: "#4caf50" }} />
					<div className="dw-stat-text">
						<span className="dw-label">
							{!today?.connected && today ? "Blitzit not connected" : `Today · ${today?.open ?? "–"}`}
						</span>
						<span className="dw-value">{today?.next ?? (today ? "All done" : "")}</span>
					</div>
				</div>
			);
		case "focus":
			return <Focus id={id} size={size} now={now} />;
		case "countdown": {
			const target = options?.date ? new Date(`${options.date}T00:00:00`) : nextNewYear(now);
			const label = options?.label || (options?.date ? "Countdown" : "New Year");
			const days = Math.max(0, Math.ceil((target.getTime() - now.getTime()) / 86_400_000));
			return size === "small" ? (
				<div className="dw-date" title={label}>
					<span className="dw-label">days</span>
					<span className="dw-big">{days}</span>
				</div>
			) : (
				<div className="dw-stat">
					<Hourglass size={16} strokeWidth={1.8} style={{ color: "#fbbf24" }} />
					<div className="dw-stat-text">
						<span className="dw-label">{label}</span>
						<span className="dw-value">{days === 0 ? "Today" : `${days} day${days === 1 ? "" : "s"}`}</span>
					</div>
				</div>
			);
		}
		case "progress": {
			const day = (now.getHours() * 60 + now.getMinutes()) / 1440;
			const start = new Date(now.getFullYear(), 0, 1).getTime();
			const year = (now.getTime() - start) / (new Date(now.getFullYear() + 1, 0, 1).getTime() - start);
			const bar = (label: string, value: number) => (
				<div className="dw-progress-row">
					<span className="dw-label">{label}</span>
					<span className="dw-meter">
						<span style={{ width: `${value * 100}%` }} />
					</span>
					{size !== "small" && <span className="dw-label">{Math.floor(value * 100)}%</span>}
				</div>
			);
			return size === "small" ? (
				<div className="dw-date" title={`Year ${Math.floor(year * 100)}%`}>
					<span className="dw-label">{now.getFullYear()}</span>
					<span className="dw-value">{Math.floor(year * 100)}%</span>
				</div>
			) : (
				<div className="dw-progress">
					{bar("Day", day)}
					{bar("Year", year)}
				</div>
			);
		}
	}
	return <Timer size={14} />;
}

function isoWeek(date: Date) {
	const d = new Date(Date.UTC(date.getFullYear(), date.getMonth(), date.getDate()));
	const day = d.getUTCDay() || 7;
	d.setUTCDate(d.getUTCDate() + 4 - day);
	const yearStart = new Date(Date.UTC(d.getUTCFullYear(), 0, 1));
	return Math.ceil(((d.getTime() - yearStart.getTime()) / 86_400_000 + 1) / 7);
}

function nextNewYear(now: Date) {
	return new Date(now.getFullYear() + 1, 0, 1);
}

/**
 * Whether a widget would look the same with `b`'s data as with `a`'s: only
 * what that kind shows is compared, so the clock's tick (every second) and
 * each metrics poll redraw only the widgets they change.
 */
function sameFor(kind: WidgetKind, a: WidgetData, b: WidgetData) {
	const minute = (d: Date) => Math.floor(d.getTime() / 60000);
	switch (kind) {
		case "weather":
			return a.weather === b.weather;
		case "battery":
			return a.metrics.battery === b.metrics.battery && a.metrics.charging === b.metrics.charging;
		case "cpu":
			return a.metrics.cpu === b.metrics.cpu;
		case "ram":
			return a.metrics.ram === b.metrics.ram;
		case "disk":
			return a.metrics.disk === b.metrics.disk;
		case "net":
			return a.metrics.down === b.metrics.down && a.metrics.up === b.metrics.up;
		case "clock":
		case "worldclock":
		case "progress":
			return minute(a.now) === minute(b.now) && a.h24 === b.h24;
		case "date":
			return a.now.toDateString() === b.now.toDateString();
		case "media":
			return a.media === b.media;
		case "tasks":
			return a.today === b.today;
		default:
			// The focus timer and countdown show seconds.
			return a.now.getTime() === b.now.getTime() && a.h24 === b.h24;
	}
}

/** A widget, redrawn only when what it shows changes. */
export const WidgetFace = memo(
	Widget,
	(a, b) =>
		a.kind === b.kind &&
		a.size === b.size &&
		a.id === b.id &&
		a.options === b.options &&
		sameFor(a.kind, a.data, b.data)
);
