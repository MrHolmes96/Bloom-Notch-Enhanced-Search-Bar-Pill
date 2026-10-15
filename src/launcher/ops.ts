import { invoke } from "@tauri-apps/api/core";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";

/** Thin wrappers around the backend commands the search bar uses. */

export const open = (path: string, verb?: string, args?: string, cwd?: string) =>
	invoke("launcher_open", { path, verb: verb ?? null, args: args ?? null, cwd: cwd ?? null });

export const openApp = (path: string) => invoke("open_app", { appName: path });

export const reveal = (path: string) => invoke("launcher_reveal", { path });

export const runCommandLine = (command: string) => invoke("launcher_run_command_line", { command });

export const copy = (text: string) => writeText(text);

export const copyFiles = (paths: string[]) => invoke("launcher_copy_files", { paths });

export const recycle = (path: string) => invoke("launcher_recycle", { path });

export const systemCommand = (command: string) => invoke("launcher_system_command", { command });

export const resolveAppPath = (path: string) =>
	invoke<string | null>("launcher_resolve_app_path", { path });

export function webUrl(template: string, text: string) {
	return template.replace(/\{q\}/g, encodeURIComponent(text));
}

export function parentFolder(path: string) {
	const trimmed = path.replace(/[\\/]+$/, "");
	const i = Math.max(trimmed.lastIndexOf("\\"), trimmed.lastIndexOf("/"));
	return i > 0 ? trimmed.slice(0, i) : trimmed;
}

export function baseName(path: string) {
	const trimmed = path.replace(/[\\/]+$/, "");
	const i = Math.max(trimmed.lastIndexOf("\\"), trimmed.lastIndexOf("/"));
	return i >= 0 ? trimmed.slice(i + 1) || trimmed : trimmed;
}

/** "Open in terminal" for a folder, with the terminal from the settings. */
export function openTerminal(folder: string, terminal: "cmd" | "powershell" | "wt") {
	if (terminal === "wt") return open("wt.exe", undefined, `-d "${folder}"`);
	if (terminal === "powershell") return open("powershell.exe", undefined, "-NoExit", folder);
	return open("cmd.exe", undefined, "/k", folder);
}
