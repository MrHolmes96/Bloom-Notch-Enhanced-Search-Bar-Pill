import { invoke } from "@tauri-apps/api/core";
import { open, reveal } from "../ops";
import type { Result, SearchContext } from "../types";

/**
 * Run Commands: what Start offers as "Run command". Programs registered
 * under App Paths and the programs, consoles (.msc) and Control Panel files
 * (.cpl) in Windows' folders, found by the name typed in Run: "regedit",
 * "dxdiag", "services.msc", "appwiz.cpl". Read once from Bloom.
 */

interface RunCommand {
	name: string;
	path: string;
}

let commands: RunCommand[] = [];
let loading: Promise<void> | null = null;

function load() {
	loading ??= invoke<RunCommand[]>("launcher_run_commands")
		.then((list) => {
			commands = list;
		})
		.catch(() => {
			loading = null;
		});
	return loading;
}
void load();

const KIND: Record<string, string> = {
	exe: "Run command",
	msc: "Console",
	cpl: "Control Panel item"
};

export function searchCommands(ctx: SearchContext, search: string): Result[] {
	const q = search.trim().toLowerCase();
	if (q.length < 2 || /\s/.test(q)) return [];
	if (!commands.length) void load();
	const out: Result[] = [];
	for (const command of commands) {
		const name = command.name.toLowerCase();
		const base = name.replace(/\.(exe|msc|cpl)$/, "");
		// As Start: the name as typed in Run, whole or its start.
		const exact = q === name || q === base;
		if (!exact && !base.startsWith(q)) continue;
		const ext = name.split(".").pop() ?? "exe";
		const key = `run:${name}`;
		out.push({
			key,
			title: base,
			subtitle: `${KIND[ext] ?? "Run command"} · ${command.path}`,
			icon: { kind: "file", path: command.path },
			// An exact name leads (as in Start); a start of one comes after apps.
			score: (exact ? 1200 : 330 - Math.min(base.length - q.length, 30)) + ctx.boost(key),
			plugin: "commands",
			run: () => open(command.path),
			runAsAdmin: () => open(command.path, "runas"),
			openLocation: () => reveal(command.path),
			copyText: command.path,
			replay: { type: "path", path: command.path, isDir: false }
		});
		if (out.length >= 8) break;
	}
	return out;
}
