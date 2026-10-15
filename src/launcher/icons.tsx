import { memo, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
	AppWindow,
	Ban,
	Bookmark,
	Calculator,
	Copy,
	DoorOpen,
	ExternalLink,
	File,
	Folder,
	Gamepad2,
	Globe,
	History,
	Info,
	KeyRound,
	Link,
	Lock,
	LogOut,
	Moon,
	Power,
	RefreshCw,
	RotateCcw,
	Search,
	Settings,
	ShieldCheck,
	SlidersHorizontal,
	Snowflake,
	SquareTerminal,
	Star,
	TextSearch,
	Trash2,
	TriangleAlert,
	Cpu,
	SquareArrowOutUpRight
} from "lucide-react";
import type { GlyphName, Icon } from "./types";

const GLYPHS: Record<GlyphName, typeof Search> = {
	admin: ShieldCheck,
	ban: Ban,
	bookmark: Bookmark,
	calculator: Calculator,
	copy: Copy,
	exit: DoorOpen,
	external: ExternalLink,
	file: File,
	folder: Folder,
	gamepad: Gamepad2,
	globe: Globe,
	hibernate: Snowflake,
	history: History,
	index: TextSearch,
	info: Info,
	keyword: KeyRound,
	link: Link,
	lock: Lock,
	logoff: LogOut,
	"open-with": SquareArrowOutUpRight,
	power: Power,
	process: Cpu,
	properties: SlidersHorizontal,
	refresh: RefreshCw,
	restart: RotateCcw,
	search: Search,
	settings: Settings,
	sleep: Moon,
	star: Star,
	terminal: SquareTerminal,
	trash: Trash2,
	warning: TriangleAlert
};

const cache = new Map<string, string | null>();
const pending = new Map<string, Promise<string | null>>();

function iconKey(icon: Icon) {
	return icon.kind === "app" ? `app:${icon.path}` : icon.kind === "file" ? `file:${icon.path}` : "";
}

function loadIcon(icon: Icon): Promise<string | null> {
	const key = iconKey(icon);
	const existing = pending.get(key);
	if (existing) return existing;
	const request = (
		icon.kind === "app"
			? invoke<string | null>("get_app_icon", { path: icon.path, name: icon.name, hwnd: null })
			: invoke<string | null>("launcher_file_icon", { path: (icon as { path: string }).path })
	)
		.catch(() => null)
		.then((data) => {
			cache.set(key, data);
			pending.delete(key);
			return data;
		});
	pending.set(key, request);
	return request;
}

export function clearIconCache() {
	cache.clear();
}

export const Glyph = ({ name, size = 18 }: { name: GlyphName; size?: number }) => {
	const Component = GLYPHS[name] ?? Search;
	return <Component size={size} strokeWidth={1.75} />;
};

/** A result's icon: the app's or file's own icon, or a glyph. */
export const ResultIcon = memo(function ResultIcon({
	icon,
	size = 28
}: {
	icon: Icon;
	size?: number;
}) {
	const key = iconKey(icon);
	const [src, setSrc] = useState<string | null | undefined>(() => (key ? cache.get(key) : null));

	useEffect(() => {
		if (!key) return;
		if (cache.has(key)) {
			setSrc(cache.get(key));
			return;
		}
		let alive = true;
		setSrc(undefined);
		void loadIcon(icon).then((data) => {
			if (alive) setSrc(data);
		});
		return () => {
			alive = false;
		};
		// The key identifies the icon.
		// eslint-disable-next-line react-hooks/exhaustive-deps
	}, [key]);

	if (icon.kind === "glyph") return <Glyph name={icon.glyph} size={Math.round(size * 0.72)} />;
	if (src) return <img src={src} alt="" width={size} height={size} draggable={false} />;
	if (src === undefined) return <span className="launcher-icon-placeholder" />;
	return icon.kind === "app" ? (
		<AppWindow size={Math.round(size * 0.72)} />
	) : (
		<File size={Math.round(size * 0.72)} />
	);
});
