import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { WidgetKind } from "./model";

/**
 * What the dock's widgets show, read only for the widgets placed: system
 * figures every 3 s, the battery as Windows reports it, music as the notch
 * hears it, and Blitzit's tasks for today.
 */

export interface Metrics {
	cpu: number | null;
	ram: number | null;
	disk: number | null;
	up: number | null;
	down: number | null;
	battery: number | null;
	charging: boolean;
}

export function useMetrics(placed: Set<WidgetKind>): Metrics {
	const [m, setM] = useState<Metrics>({
		cpu: null,
		ram: null,
		disk: null,
		up: null,
		down: null,
		battery: null,
		charging: false
	});
	const wanted = ["cpu", "ram", "disk", "net"].filter((k) => placed.has(k as WidgetKind)).join(",");
	useEffect(() => {
		if (!wanted) return;
		const kinds = new Set(wanted.split(","));
		const read = () => {
			if (kinds.has("cpu"))
				invoke<number>("get_cpu_usage")
					.then((cpu) => setM((p) => ({ ...p, cpu })))
					.catch(() => {});
			if (kinds.has("ram"))
				invoke<number>("get_ram_usage")
					.then((ram) => setM((p) => ({ ...p, ram })))
					.catch(() => {});
			if (kinds.has("disk"))
				invoke<number>("get_disk_space")
					.then((disk) => setM((p) => ({ ...p, disk })))
					.catch(() => {});
			if (kinds.has("net"))
				invoke<[number, number]>("get_network_speed")
					.then(([up, down]) => setM((p) => ({ ...p, up, down })))
					.catch(() => {});
		};
		read();
		const timer = window.setInterval(read, 3000);
		return () => window.clearInterval(timer);
	}, [wanted]);

	const battery = placed.has("battery");
	useEffect(() => {
		if (!battery) return;
		let source: any = null;
		const update = () =>
			setM((p) => ({
				...p,
				battery: Math.round(source.level * 100),
				charging: !!source.charging
			}));
		void (navigator as any)
			.getBattery?.()
			.then((b: any) => {
				source = b;
				update();
				b.addEventListener("levelchange", update);
				b.addEventListener("chargingchange", update);
			})
			.catch(() => {});
		return () => {
			source?.removeEventListener("levelchange", update);
			source?.removeEventListener("chargingchange", update);
		};
	}, [battery]);
	return m;
}

export interface Media {
	title: string;
	artist: string;
	is_playing: boolean;
	has_media: boolean;
	artwork?: string[];
}

export function useMedia(on: boolean): Media | null {
	const [media, setMedia] = useState<Media | null>(null);
	useEffect(() => {
		if (!on) return;
		void invoke<Media | null>("media_now")
			.then((m) => m && setMedia(m))
			.catch(() => {});
		const unlisten = listen<Media>("media-update", (e) => setMedia(e.payload));
		return () => {
			void unlisten.then((f) => f());
		};
	}, [on]);
	return media;
}

export interface TodayTasks {
	open: number;
	next: string | null;
	connected: boolean;
}

/** Blitzit's Today column (all lists), as the notch's board has it. */
export function useTodayTasks(on: boolean): TodayTasks | null {
	const [today, setToday] = useState<TodayTasks | null>(null);
	useEffect(() => {
		if (!on) return;
		const load = () =>
			void invoke<{
				connected: boolean;
				tasks: { name: string; done: boolean; column: string }[];
			}>("blitzit_board", { listId: "__all__", fresh: false })
				.then((board) => {
					const open = board.tasks.filter(
						(t) => !t.done && t.column.toLowerCase().replace(/[^a-z]/g, "").includes("today")
					);
					setToday({ open: open.length, next: open[0]?.name ?? null, connected: board.connected });
				})
				.catch(() => {});
		load();
		const timer = window.setInterval(load, 30_000);
		const changed = listen("blitzit-changed", load);
		return () => {
			window.clearInterval(timer);
			void changed.then((f) => f());
		};
	}, [on]);
	return today;
}

/** The time, ticking every `ms` while `on`. */
export function useNow(on: boolean, ms = 1000): Date {
	const [now, setNow] = useState(() => new Date());
	useEffect(() => {
		if (!on) return;
		const timer = window.setInterval(() => setNow(new Date()), ms);
		return () => window.clearInterval(timer);
	}, [on, ms]);
	return now;
}

export function speed(bytes: number | null) {
	if (bytes === null) return "–";
	if (bytes < 1024) return `${Math.round(bytes)} B/s`;
	if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB/s`;
	return `${(bytes / (1024 * 1024)).toFixed(1)} MB/s`;
}
