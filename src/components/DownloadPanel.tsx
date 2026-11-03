import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ShownProgress } from "../dockProgress";
import "./DownloadPanel.css";

/**
 * Under a dock preview: an app's download, as the app shows it where Bloom
 * can read its numbers (Steam's game and sizes, a browser's file), else its
 * stage, how far it is and the time left (see `download_details.rs`).
 */

interface Details {
	stage: "downloading" | "installing" | "paused" | "error";
	/** Whose numbers these are. */
	source: "steam" | "browser" | null;
	value: number | null;
	name: string | null;
	done: number | null;
	total: number | null;
	/** Bytes per second. */
	speed: number | null;
	/** Seconds. */
	time_left: number | null;
	/** Steam's "Updating files", 0 to 1. */
	updating: number | null;
}

const number = new Intl.NumberFormat(undefined, { maximumFractionDigits: 1, minimumFractionDigits: 1 });

function size(bytes: number) {
	const units = ["B", "KB", "MB", "GB", "TB"];
	let value = bytes;
	let unit = 0;
	while (value >= 1024 && unit < units.length - 1) {
		value /= 1024;
		unit += 1;
	}
	return `${unit === 0 ? Math.round(value) : number.format(value)} ${units[unit]}`;
}

/** "14:55" or "1:05:20", as Steam writes the time left. */
function clock(seconds: number) {
	const total = Math.max(1, Math.round(seconds));
	const h = Math.floor(total / 3600);
	const m = Math.floor((total % 3600) / 60);
	const s = total % 60;
	const two = (n: number) => String(n).padStart(2, "0");
	return h > 0 ? `${h}:${two(m)}:${two(s)}` : `${m}:${two(s)}`;
}

const percent = (value: number) => `${Math.floor(value * 100)}%`;

function Bar({ value, tone }: { value: number | null; tone: "accent" | "green" }) {
	return (
		<div className={`dl-bar dl-bar--${tone}${value === null ? " dl-bar--sweep" : ""}`}>
			<div className="dl-bar-fill" style={value === null ? undefined : { width: `${value * 100}%` }} />
		</div>
	);
}

export function DownloadPanel({
	hwnd,
	progress,
	onLayout
}: {
	hwnd: number;
	progress: ShownProgress;
	/** Called when the panel's size changes (the dock's clickable area follows). */
	onLayout?: () => void;
}) {
	const [details, setDetails] = useState<Details | null>(null);
	const ref = useRef<HTMLDivElement | null>(null);

	useEffect(() => {
		let alive = true;
		const load = () =>
			void invoke<Details | null>("download_details", { hwnd })
				.then((d) => alive && setDetails(d))
				.catch(() => {});
		load();
		const timer = window.setInterval(load, 1000);
		return () => {
			alive = false;
			window.clearInterval(timer);
		};
	}, [hwnd]);

	const height = ref.current?.offsetHeight ?? 0;
	useLayoutEffect(() => {
		onLayout?.();
		// eslint-disable-next-line react-hooks/exhaustive-deps
	}, [height, details === null]);

	// Until the first answer: what the dock already knows.
	const stage: Details["stage"] =
		details?.stage ?? (progress.state === "normal" || progress.state === "indeterminate" ? "downloading" : progress.state);
	const value = details ? details.value : progress.value;
	const label =
		stage === "installing"
			? "Installing"
			: stage === "paused"
				? "Paused"
				: stage === "error"
					? "Failed"
					: details?.source === "steam"
						? "Downloading data"
						: "Downloading";
	const amount =
		details?.done != null && details.total != null && stage !== "installing"
			? `${size(details.done)} / ${size(details.total)}`
			: details?.done != null && stage !== "installing"
				? size(details.done)
				: value !== null
					? percent(value)
					: "";

	return (
		<div className="dl-panel" ref={ref}>
			{details?.name && <div className="dl-name">{details.name}</div>}
			<div className="dl-row">
				<span className="dl-label">{label}</span>
				<span className="dl-amount">{amount}</span>
			</div>
			<Bar value={value} tone={stage === "installing" ? "green" : "accent"} />
			{details?.updating != null && (
				<>
					<div className="dl-row">
						<span className="dl-label">Updating files</span>
						<span className="dl-amount">{percent(details.updating)}</span>
					</div>
					<Bar value={details.updating} tone="green" />
				</>
			)}
			{(details?.speed != null || details?.time_left != null) && stage !== "paused" && stage !== "error" && (
				<div className="dl-row dl-foot">
					<span>{details?.speed != null ? `${size(details.speed)}/s` : ""}</span>
					<span>{details?.time_left != null ? `${clock(details.time_left)} remaining` : ""}</span>
				</div>
			)}
		</div>
	);
}
