import { invoke } from "@tauri-apps/api/core";
import { bestMatch } from "../fuzzy";
import { recordShell, shellHistory } from "../history";
import { copy, reveal } from "../ops";
import type { Result, SearchContext } from "../types";

const SHELL_NAMES = {
	cmd: "Command Prompt",
	powershell: "PowerShell",
	pwsh: "PowerShell 7",
	run: "the Run dialog"
} as const;

export function runShell(ctx: SearchContext, command: string, admin: boolean) {
	const { shell } = ctx.config;
	recordShell(command);
	return invoke("launcher_run_shell", {
		command,
		shell: shell.shell,
		after: shell.after,
		admin: admin || shell.admin,
		terminal: shell.terminal,
		cwd: null
	});
}

function shellResult(
	ctx: SearchContext,
	command: string,
	score: number,
	usedTimes?: number
): Result {
	const where = SHELL_NAMES[ctx.config.shell.shell];
	return {
		key: `shell:${command}`,
		title: command,
		subtitle: usedTimes
			? `Run in ${where} · used ${usedTimes} time${usedTimes === 1 ? "" : "s"}`
			: `Run in ${where} · Ctrl+Shift+Enter for administrator`,
		icon: { kind: "glyph", glyph: "terminal" },
		score,
		plugin: "shell",
		run: () => runShell(ctx, command, false),
		runAsAdmin: () => runShell(ctx, command, true),
		autoComplete: command,
		copyText: command,
		actions: () => [
			{ key: "run", title: "Run", glyph: "terminal", run: () => runShell(ctx, command, false) },
			{
				key: "admin",
				title: "Run as administrator",
				subtitle: "Ctrl+Shift+Enter",
				glyph: "admin",
				run: () => runShell(ctx, command, true)
			},
			{ key: "copy", title: "Copy command", glyph: "copy", run: () => copy(command) }
		],
		replay: { type: "shell", command }
	};
}

/** "> ipconfig": run it, or pick an earlier command. */
export function searchShell(ctx: SearchContext, search: string): Result[] {
	const command = search.trim();
	const out: Result[] = [];
	if (command) out.push(shellResult(ctx, command, 2000));
	const seen = new Set([command.toLowerCase()]);
	for (const { command: past, count } of shellHistory()) {
		if (seen.has(past.toLowerCase())) continue;
		const score = command ? bestMatch(command, [past]) : 500;
		if (command && score < ctx.min) continue;
		seen.add(past.toLowerCase());
		out.push(shellResult(ctx, past, 1000 + score * 0.3 + Math.min(count, 50), count));
		if (out.length > ctx.config.shell.historyCount) break;
	}
	return out;
}

interface ProcessEntry {
	pid: number;
	name: string;
	path: string | null;
	title: string | null;
}

let processCache: { at: number; list: ProcessEntry[] } | null = null;

export function clearProcessCache() {
	processCache = null;
}

async function processes(): Promise<ProcessEntry[]> {
	if (processCache && Date.now() - processCache.at < 3000) return processCache.list;
	const list = await invoke<ProcessEntry[]>("launcher_processes").catch(() => []);
	processCache = { at: Date.now(), list };
	return list;
}

async function kill(ctx: SearchContext, pids: number[], label: string) {
	const failed = await invoke<number>("launcher_kill", { pids });
	clearProcessCache();
	ctx.notify(
		failed === 0
			? `Ended ${label}`
			: failed === pids.length
				? `Couldn't end ${label}. It may be running as administrator`
				: `Ended ${pids.length - failed} of ${pids.length} (${label})`
	);
	ctx.requery();
}

/** "kill chrome": end a process, or all of its instances. */
export async function searchProcesses(
	ctx: SearchContext,
	search: string,
	isCurrent: () => boolean
): Promise<Result[]> {
	const q = search.trim();
	const list = await processes();
	if (!isCurrent()) return [];
	const { showTitle, windowsFirst } = ctx.config.processes;
	const matches = list
		.map((p) => ({
			p,
			score: q ? bestMatch(q, [p.name.replace(/\.exe$/i, ""), p.title ?? undefined], [1, 0.8]) : 500
		}))
		.filter((m) => !q || m.score >= ctx.min);

	const out: Result[] = [];
	const groups = new Map<string, ProcessEntry[]>();
	for (const { p } of matches) {
		const key = p.name.toLowerCase();
		groups.set(key, [...(groups.get(key) ?? []), p]);
	}
	if (q) {
		for (const [, group] of groups) {
			if (group.length < 2) continue;
			const name = group[0].name;
			out.push({
				key: `kill-all:${name.toLowerCase()}`,
				title: `End all ${name} processes`,
				subtitle: `${group.length} running`,
				icon: { kind: "glyph", glyph: "process" },
				score: 2000,
				plugin: "processes",
				keepOpen: true,
				noHistory: true,
				run: () =>
					kill(
						ctx,
						group.map((g) => g.pid),
						name
					)
			});
		}
	}
	for (const { p, score } of matches) {
		const label = `${p.name} (${p.pid})`;
		out.push({
			key: `kill:${p.pid}`,
			title: label,
			subtitle: (showTitle && p.title) || p.path || "",
			icon: p.path ? { kind: "file", path: p.path } : { kind: "glyph", glyph: "process" },
			score: 1000 + score * 0.5 + (windowsFirst && p.title ? 400 : 0),
			plugin: "processes",
			keepOpen: true,
			noHistory: true,
			run: () => kill(ctx, [p.pid], label),
			copyText: p.path ?? p.name,
			actions: () => [
				{
					key: "kill",
					title: "End process",
					glyph: "process",
					keepOpen: true,
					run: () => kill(ctx, [p.pid], label)
				},
				...(p.path
					? [
							{
								key: "location",
								title: "Open file location",
								glyph: "folder" as const,
								run: () => reveal(p.path!)
							},
							{ key: "copy", title: "Copy path", glyph: "copy" as const, run: () => copy(p.path!) }
						]
					: [])
			]
		});
	}
	return out;
}
