import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Check, Globe, Laptop, Loader2, Monitor, RotateCw, Send, Server, Smartphone, X } from "lucide-react";

/** A nearby device running LocalSend (see `localsend.rs`). */
export interface LocalSendDevice {
	alias: string;
	deviceModel: string | null;
	deviceType: string;
	ip: string;
	port: number;
	https: boolean;
}

export interface LocalSendProgress {
	state: "waiting" | "sending" | "done" | "declined" | "failed" | "stopped";
	sent: number;
	total: number;
	device: string;
	message: string;
}

/** Something from the shelf to send. */
export interface LocalSendItem {
	path?: string;
	text?: string;
}

/**
 * LocalSend for the shelf: nearby devices while `searching` is on (only
 * then; nothing runs otherwise), and the progress of a send, which goes on
 * when the notch closes.
 */
export function useLocalSend(searching: boolean) {
	const [devices, setDevices] = useState<LocalSendDevice[]>([]);
	const [looking, setLooking] = useState(false);
	const [progress, setProgress] = useState<LocalSendProgress | null>(null);
	const [error, setError] = useState<string | null>(null);

	useEffect(() => {
		const unlisten = listen<LocalSendProgress>("localsend-progress", (e) => setProgress(e.payload));
		return () => {
			void unlisten.then((fn) => fn());
		};
	}, []);

	const find = useCallback(() => {
		setLooking(true);
		setError(null);
		void invoke<LocalSendDevice[]>("localsend_find_devices")
			.then((found) => setDevices(found))
			.catch((e) => {
				setLooking(false);
				setError(String(e));
			});
	}, []);

	useEffect(() => {
		if (!searching) return;
		setDevices([]);
		const unlistenDevice = listen<LocalSendDevice>("localsend-device", (e) =>
			setDevices((list) => (list.some((d) => d.ip === e.payload.ip) ? list : [...list, e.payload]))
		);
		const unlistenDone = listen<string>("localsend-search-done", (e) => {
			setLooking(false);
			if (e.payload) setError(e.payload);
		});
		find();
		return () => {
			void unlistenDevice.then((fn) => fn());
			void unlistenDone.then((fn) => fn());
			setLooking(false);
			void invoke("localsend_stop_finding").catch(() => {});
		};
	}, [searching, find]);

	const send = useCallback((device: LocalSendDevice, items: LocalSendItem[]) => {
		setProgress({ state: "waiting", sent: 0, total: 0, device: device.alias, message: "" });
		return invoke("localsend_send", { device, items }).then(
			() => true,
			(e) => {
				setProgress((p) =>
					p && p.state !== "waiting" && p.state !== "sending"
						? p
						: { state: "failed", sent: 0, total: 0, device: device.alias, message: String(e) }
				);
				return false;
			}
		);
	}, []);

	const cancel = useCallback(() => {
		void invoke("localsend_cancel").catch(() => {});
	}, []);

	const clearProgress = useCallback(() => setProgress(null), []);

	return { devices, looking, progress, error, find, send, cancel, clearProgress };
}

const DEVICE_ICONS: Record<string, typeof Smartphone> = {
	mobile: Smartphone,
	desktop: Monitor,
	web: Globe,
	headless: Server,
	server: Server
};

function deviceIcon(device: LocalSendDevice) {
	if (device.deviceType === "desktop" && /mac|laptop|book/i.test(device.deviceModel ?? "")) return Laptop;
	return DEVICE_ICONS[device.deviceType] ?? Monitor;
}

function sizeText(bytes: number) {
	if (bytes < 1024) return `${bytes} B`;
	if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
	if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
	return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

/**
 * The shelf's LocalSend row while picking: what is picked, the devices
 * found, and then the send's progress.
 */
export function LocalSendBar({
	picked,
	devices,
	looking,
	progress,
	error,
	onSend,
	onRetry,
	onCancel,
	onClose
}: {
	picked: number;
	devices: LocalSendDevice[];
	looking: boolean;
	progress: LocalSendProgress | null;
	error: string | null;
	onSend: (device: LocalSendDevice) => void;
	onRetry: () => void;
	onCancel: () => void;
	onClose: () => void;
}) {
	const active = progress && (progress.state === "waiting" || progress.state === "sending");
	const chipsRef = useRef<HTMLDivElement>(null);

	let status: React.ReactNode;
	if (progress) {
		const percent = progress.total > 0 ? Math.round((progress.sent / progress.total) * 100) : 0;
		status =
			progress.state === "waiting" ? (
				<span className="localsend-status">
					<Loader2 size={12} className="localsend-spin" /> Waiting for {progress.device} to accept…
				</span>
			) : progress.state === "sending" ? (
				<span className="localsend-status localsend-sending">
					<span className="localsend-meter">
						<span style={{ width: `${percent}%` }} />
					</span>
					{percent}% · {sizeText(progress.sent)} of {sizeText(progress.total)}
				</span>
			) : progress.state === "done" ? (
				<span className="localsend-status localsend-done">
					<Check size={12} strokeWidth={2.6} /> Sent to {progress.device}
				</span>
			) : (
				<span className="localsend-status localsend-failed" title={progress.message}>
					{progress.state === "stopped" ? "Stopped" : progress.message || "Couldn't send"}
				</span>
			);
	}

	return (
		<div className="localsend-bar" onClick={(e) => e.stopPropagation()}>
			<div className="localsend-head">
				<Send size={11} strokeWidth={2.4} />
				<span>{picked === 0 ? "Pick files to send" : `${picked} picked`}</span>
			</div>
			{status ?? (
				<div
					className="localsend-devices"
					ref={chipsRef}
					onWheel={(e) => {
						// The wheel scrolls the chips sideways, not the notch's pages.
						e.stopPropagation();
						if (chipsRef.current) chipsRef.current.scrollLeft += e.deltaY;
					}}
				>
					{devices.map((device) => {
						const Icon = deviceIcon(device);
						return (
							<button
								key={device.ip}
								type="button"
								className="localsend-device"
								disabled={picked === 0}
								title={
									picked === 0
										? "Pick files first"
										: `Send to ${device.alias}${device.deviceModel ? ` (${device.deviceModel})` : ""}`
								}
								onClick={() => onSend(device)}
							>
								<Icon size={12} strokeWidth={2} />
								<span>{device.alias}</span>
							</button>
						);
					})}
					{looking && (
						<span className="localsend-looking">
							<Loader2 size={12} className="localsend-spin" />
							{devices.length === 0 ? "Looking for devices…" : ""}
						</span>
					)}
					{!looking && devices.length === 0 && (
						<button type="button" className="localsend-retry" onClick={onRetry}>
							<RotateCw size={11} strokeWidth={2.4} />
							{error || "No devices found. Open LocalSend on the other device"}
						</button>
					)}
				</div>
			)}
			{active ? (
				<button type="button" className="localsend-close" title="Stop sending" onClick={onCancel}>
					<X size={12} strokeWidth={2.4} />
				</button>
			) : (
				<button type="button" className="localsend-close" title="Close LocalSend" onClick={onClose}>
					<X size={12} strokeWidth={2.4} />
				</button>
			)}
		</div>
	);
}
