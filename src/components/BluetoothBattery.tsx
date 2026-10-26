import {
	Bluetooth,
	Gamepad2,
	Headphones,
	Headset,
	Keyboard,
	Laptop,
	Mouse,
	Smartphone,
	Speaker,
	Watch
} from "lucide-react";

/** A paired Bluetooth device as Bloom reports it (bluetooth.rs, BtDevice). */
export interface BtDeviceInfo {
	id: string;
	name: string;
	kind: string;
	connected: boolean;
	battery: number | null;
	charging: boolean | null;
	can_connect: boolean;
}

/** Earbuds: two buds, which no icon set has. Drawn like lucide's icons. */
function Earbuds({ size = 24, strokeWidth = 2 }: { size?: number; strokeWidth?: number }) {
	return (
		<svg
			width={size}
			height={size}
			viewBox="0 0 24 24"
			fill="none"
			stroke="currentColor"
			strokeWidth={strokeWidth}
			strokeLinecap="round"
			strokeLinejoin="round"
			aria-hidden="true"
		>
			<path d="M6.5 4a3.5 3.5 0 0 1 3.5 3.5V9a3 3 0 0 1-3 3H6.5a3.5 3.5 0 0 1 0-7z" />
			<path d="M8 12v7a1.5 1.5 0 0 1-3 0v-6" />
			<path d="M17.5 4A3.5 3.5 0 0 0 14 7.5V9a3 3 0 0 0 3 3h.5a3.5 3.5 0 0 0 0-7z" />
			<path d="M16 12v7a1.5 1.5 0 0 0 3 0v-6" />
		</svg>
	);
}

/** The icon for a kind of device: a controller, earbuds, a headset… */
export function DeviceKindIcon({
	kind,
	size = 24,
	strokeWidth = 2
}: {
	kind: string;
	size?: number;
	strokeWidth?: number;
}) {
	const props = { size, strokeWidth };
	switch (kind) {
		case "earbuds":
			return <Earbuds {...props} />;
		case "headset":
			return <Headset {...props} />;
		case "headphones":
		case "audio":
			return <Headphones {...props} />;
		case "speaker":
			return <Speaker {...props} />;
		case "phone":
			return <Smartphone {...props} />;
		case "computer":
			return <Laptop {...props} />;
		case "keyboard":
			return <Keyboard {...props} />;
		case "mouse":
			return <Mouse {...props} />;
		case "gamepad":
			return <Gamepad2 {...props} />;
		case "wearable":
			return <Watch {...props} />;
		default:
			return <Bluetooth {...props} />;
	}
}

/**
 * A battery like the laptop's in the notch: its level, red at or below the
 * low-battery level, green with a bolt while charging. Unknown: an outline.
 */
export function MiniBattery({
	level,
	charging,
	low = 20
}: {
	level: number | null;
	charging?: boolean | null;
	low?: number;
}) {
	const known = level !== null && level !== undefined;
	const percent = known ? Math.min(Math.max(level, 0), 100) : 0;
	const fill = charging ? "#32D74B" : percent <= low ? "#FF453A" : "currentColor";
	return (
		<span className="mini-battery" title={known ? `${percent}%${charging ? ", charging" : ""}` : "Battery unknown"}>
			<svg width="20" height="10" viewBox="0 0 20 10" fill="none" aria-hidden="true">
				<rect
					x="2"
					y="0.75"
					width="14"
					height="8.5"
					rx="2.4"
					stroke="currentColor"
					strokeOpacity={0.35}
					strokeWidth="1.1"
				/>
				<path
					d="M17.5 3.5V6.5"
					stroke="currentColor"
					strokeOpacity={0.35}
					strokeWidth="1.2"
					strokeLinecap="round"
				/>
				{known && (
					<rect
						x="3.8"
						y="2.5"
						width={Math.max(0.5, (percent / 100) * 10.4)}
						height="5"
						rx="1"
						fill={fill}
					/>
				)}
			</svg>
			{charging && (
				<svg
					className="mini-battery-bolt"
					width="7"
					height="10"
					viewBox="0 0 8 12"
					fill="white"
					aria-hidden="true"
				>
					<path d="M4.5 0L0 7H3.5L2.5 12L8 5H4.5L5.5 0H4.5Z" />
				</svg>
			)}
		</span>
	);
}
