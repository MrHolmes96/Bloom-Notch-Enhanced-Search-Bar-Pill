import { invoke } from "@tauri-apps/api/core";
import { evaluate, formatResult, systemDecimalMode } from "../calc";
import { bestMatch } from "../fuzzy";
import { PLUGINS } from "../config";
import { copy, runCommandLine, systemCommand } from "../ops";
import { WINDOWS_SETTINGS } from "../windowsSettings";
import type { GlyphName, Result, SearchContext } from "../types";

export function searchCalculator(ctx: SearchContext, search: string): Result[] {
	const { calculator } = ctx.config;
	const mode = calculator.decimal === "system" ? systemDecimalMode() : calculator.decimal;
	const value = evaluate(search, mode);
	if (value === null) return [];
	const text = formatResult(value, {
		mode,
		maxDecimals: calculator.maxDecimals,
		thousands: calculator.thousands
	});
	return [
		{
			key: "calc",
			title: text,
			subtitle: "Copy this number to the clipboard",
			icon: { kind: "glyph", glyph: "calculator" },
			score: 1500,
			plugin: "calculator",
			noHistory: true,
			copyText: text,
			run: () => copy(text),
			actions: () => [
				{ key: "copy", title: "Copy result", glyph: "copy", run: () => copy(text) },
				{
					key: "copy-raw",
					title: "Copy without formatting",
					subtitle: String(value),
					glyph: "copy",
					run: () => copy(String(value))
				}
			]
		}
	];
}

const KIND_LABEL = { A: "Settings", C: "Control Panel", T: "Control Panel" } as const;

export function searchWindowsSettings(ctx: SearchContext, search: string): Result[] {
	const q = search.trim();
	if (!q) return [];
	const out: Result[] = [];
	for (const [name, area, kind, command, extra] of WINDOWS_SETTINGS) {
		const score = bestMatch(q, [name, extra, area], [1, 0.75, 0.5]);
		if (score < ctx.min) continue;
		const key = `setting:${command}`;
		out.push({
			key,
			title: name,
			subtitle: area ? `${area} · ${KIND_LABEL[kind]}` : KIND_LABEL[kind],
			icon: { kind: "glyph", glyph: "settings" },
			score: 300 + score * 0.5 + ctx.boost(key),
			plugin: "settings",
			run: () => runCommandLine(command),
			copyText: command,
			actions: () => [
				{ key: "open", title: "Open", glyph: "external", run: () => runCommandLine(command) },
				{
					key: "copy",
					title: "Copy command",
					subtitle: command,
					glyph: "copy",
					run: () => copy(command)
				}
			],
			replay: { type: "command-line", command }
		});
	}
	return out;
}

interface SystemCommand {
	id: string;
	title: string;
	subtitle: string;
	glyph: GlyphName;
	words?: string;
	/** Asks for a second Enter when "Confirm system commands" is on. */
	risky?: boolean;
	run: (ctx: SearchContext) => unknown;
	keepOpen?: boolean;
}

export const SYSTEM_COMMANDS: SystemCommand[] = [
	{
		id: "shutdown",
		title: "Shut down",
		subtitle: "Shut down the computer",
		glyph: "power",
		words: "shutdown power off turn off",
		risky: true,
		run: () => systemCommand("shutdown")
	},
	{
		id: "restart",
		title: "Restart",
		subtitle: "Restart the computer",
		glyph: "restart",
		words: "reboot",
		risky: true,
		run: () => systemCommand("restart")
	},
	{
		id: "restart-advanced",
		title: "Restart with advanced boot options",
		subtitle: "Restart into safe mode, recovery and other startup options",
		glyph: "restart",
		words: "safe mode recovery bios uefi",
		risky: true,
		run: () => systemCommand("restart-advanced")
	},
	{
		id: "logoff",
		title: "Sign out",
		subtitle: "Sign out of Windows",
		glyph: "logoff",
		words: "log off logoff logout",
		risky: true,
		run: () => systemCommand("logoff")
	},
	{
		id: "lock",
		title: "Lock",
		subtitle: "Lock this computer",
		glyph: "lock",
		run: () => systemCommand("lock")
	},
	{
		id: "sleep",
		title: "Sleep",
		subtitle: "Put the computer to sleep",
		glyph: "sleep",
		words: "suspend",
		run: () => systemCommand("sleep")
	},
	{
		id: "hibernate",
		title: "Hibernate",
		subtitle: "Hibernate the computer",
		glyph: "hibernate",
		run: () => systemCommand("hibernate")
	},
	{
		id: "empty-recycle-bin",
		title: "Empty Recycle Bin",
		subtitle: "Permanently delete everything in the Recycle Bin",
		glyph: "trash",
		words: "trash",
		run: () => systemCommand("empty-recycle-bin")
	},
	{
		id: "open-recycle-bin",
		title: "Open Recycle Bin",
		subtitle: "Show what's in the Recycle Bin",
		glyph: "trash",
		words: "trash",
		run: () => systemCommand("open-recycle-bin")
	},
	{
		id: "indexing-options",
		title: "Indexing Options",
		subtitle: "Choose which folders Windows Search covers",
		glyph: "index",
		words: "index search",
		run: () => systemCommand("indexing-options")
	},
	{
		id: "bloom-settings",
		title: "Search Bar Pill settings",
		subtitle: "Change how the search bar works",
		glyph: "settings",
		words: "bloom preferences options launcher",
		run: (ctx) => ctx.openSettings()
	},
	{
		id: "reload",
		title: "Reload search data",
		subtitle: "Rescan apps, bookmarks and processes (F5)",
		glyph: "refresh",
		words: "refresh reindex",
		keepOpen: true,
		run: (ctx) => {
			ctx.reload();
			ctx.notify("Search data reloaded");
		}
	},
	{
		id: "game-mode",
		title: "Toggle game mode",
		subtitle: "Ignore the search bar's shortcuts until you turn it off again",
		glyph: "gamepad",
		words: "gaming hotkey",
		run: async (ctx) => {
			const now = await invoke<boolean>("launcher_game_mode");
			await invoke("launcher_set_game_mode", { enabled: !now });
			ctx.notify(!now ? "Game mode on" : "Game mode off");
		}
	},
	{
		id: "bloom-data",
		title: "Bloom settings folder",
		subtitle: "Open the folder where Bloom keeps its settings",
		glyph: "folder",
		words: "userdata config",
		run: () => invoke("launcher_open_config_dir")
	},
	{
		id: "bloom-restart",
		title: "Restart Bloom",
		subtitle: "Restart Bloom (notch, dock and search bar)",
		glyph: "restart",
		run: () => invoke("restart_bloom")
	},
	{
		id: "bloom-exit",
		title: "Exit Bloom",
		subtitle: "Close Bloom completely",
		glyph: "exit",
		words: "quit close",
		risky: true,
		run: () => invoke("quit_bloom")
	}
];

export function systemResult(ctx: SearchContext, cmd: SystemCommand, score: number): Result {
	const key = `system:${cmd.id}`;
	return {
		key,
		title: cmd.title,
		subtitle: cmd.subtitle,
		icon: { kind: "glyph", glyph: cmd.glyph },
		score: score + ctx.boost(key),
		plugin: "system",
		keepOpen: cmd.keepOpen,
		confirm:
			cmd.risky && ctx.config.system.confirm
				? `Press Enter again to ${cmd.title.toLowerCase()}`
				: undefined,
		run: () => cmd.run(ctx),
		replay: cmd.risky || cmd.keepOpen ? undefined : { type: "system", id: cmd.id }
	};
}

export function searchSystem(ctx: SearchContext, search: string): Result[] {
	const q = search.trim();
	if (!q) return [];
	const out: Result[] = [];
	for (const cmd of SYSTEM_COMMANDS) {
		const score = bestMatch(q, [cmd.title, cmd.words], [1, 0.85]);
		if (score >= ctx.min) out.push(systemResult(ctx, cmd, 350 + score * 0.6));
	}
	return out;
}

/** "?" lists the keywords you can type; picking one fills it in. */
export function searchIndicator(ctx: SearchContext, search: string): Result[] {
	const q = search.trim().toLowerCase();
	const rows: { keyword: string; title: string }[] = [];
	for (const plugin of PLUGINS) {
		const setting = ctx.config.plugins[plugin.id];
		if (
			!setting.enabled ||
			!plugin.hasKeyword ||
			setting.keyword === "*" ||
			plugin.id === "indicator"
		)
			continue;
		rows.push({ keyword: setting.keyword, title: plugin.name });
	}
	if (ctx.config.plugins.explorer.enabled && ctx.config.explorer.contentEnabled) {
		rows.push({ keyword: ctx.config.explorer.contentKeyword, title: "Search inside files" });
	}
	if (ctx.config.plugins.web.enabled) {
		for (const source of ctx.config.web.sources) {
			if (source.enabled && source.keyword !== "*")
				rows.push({ keyword: source.keyword, title: source.title });
		}
	}
	return rows
		.filter((r) => !q || r.keyword.toLowerCase().startsWith(q) || r.title.toLowerCase().includes(q))
		.map((r, i) => ({
			key: `keyword:${r.keyword}`,
			title: r.keyword,
			subtitle: `Search ${r.title}`,
			icon: { kind: "glyph", glyph: "keyword" },
			score: 1300 - i,
			plugin: "indicator",
			keepOpen: true,
			noHistory: true,
			run: () => ctx.setQuery(`${r.keyword} `)
		}));
}
