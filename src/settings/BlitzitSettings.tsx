import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import {
	ArrowLeftRight,
	CheckSquare,
	ClipboardCopy,
	Link2,
	ListChecks,
	ListTodo,
	PanelTop,
	Plus,
	RefreshCw,
	Rocket,
	Server,
	TriangleAlert
} from "lucide-react";
import { SettingRow } from "./SettingRow";
import { useStoredFlag, useStoredString } from "./useStoredSetting";
import type { BlitzitStatus } from "../components/BlitzitPanel";

/**
 * Settings > Integrations > Blitzit: the account, the notch page, and which
 * of Blitzit's actions Bloom uses. Bloom picks them by itself; the choice can
 * be made by hand when Blitzit names them in a way Bloom doesn't recognise.
 */

const SERVERS = [
	{ url: "https://core.blitzit.app/v3/mcp", label: "Blitzit 3 and newer" },
	{ url: "https://integration-prd.blitzit.app/mcp", label: "Older Blitzit" }
];

const ACTIONS = [
	{ id: "create", label: "Add a Task", icon: Plus },
	{ id: "lists", label: "Your Lists", icon: ListTodo },
	{ id: "tasks", label: "A List's Tasks", icon: ListChecks },
	{ id: "complete", label: "Tick a Task Off", icon: CheckSquare },
	{ id: "move", label: "Move a Task to Another Column", icon: ArrowLeftRight }
] as const;

function parseChoices(raw: string): Record<string, string> {
	try {
		const parsed: unknown = JSON.parse(raw || "{}");
		return parsed && typeof parsed === "object" ? (parsed as Record<string, string>) : {};
	} catch {
		return {};
	}
}

export function BlitzitSettings() {
	const [status, setStatus] = useState<BlitzitStatus | null>(null);
	const [checking, setChecking] = useState(false);
	const [connecting, setConnecting] = useState(false);
	const [problem, setProblem] = useState<string | null>(null);
	const [copied, setCopied] = useState<string | null>(null);
	const [inScroll, setInScroll] = useStoredFlag("bloom-notch-blitzit", true);
	const [server, setServer] = useStoredString("bloom-blitzit-server", SERVERS[0].url);
	const [choicesRaw, setChoicesRaw] = useStoredString("bloom-blitzit-tools", "{}");
	const choices = parseChoices(choicesRaw);

	const refresh = useCallback(async () => {
		setChecking(true);
		try {
			const next = await invoke<BlitzitStatus>("blitzit_status", { check: true });
			setStatus(next);
			setProblem(next.problem);
		} catch (e) {
			setProblem(String(e));
		} finally {
			setChecking(false);
		}
	}, []);

	useEffect(() => {
		void refresh();
		const changed = listen("blitzit-changed", () => void refresh());
		return () => {
			void changed.then((f) => f());
		};
	}, [refresh]);

	const connect = async () => {
		setConnecting(true);
		setProblem(null);
		try {
			const next = await invoke<BlitzitStatus>("blitzit_connect");
			setStatus(next);
			setProblem(next.problem);
		} catch (e) {
			setProblem(String(e));
		} finally {
			setConnecting(false);
		}
	};

	const choose = (id: string, tool: string) => {
		const next = { ...choices };
		if (tool) next[id] = tool;
		else delete next[id];
		setChoicesRaw(JSON.stringify(next));
		// The notch picks the change up on its next refresh.
		window.setTimeout(() => void refresh(), 150);
	};

	const copyReport = async () => {
		try {
			await writeText(await invoke<string>("blitzit_tools_report"));
			setCopied("Copied. Paste it to whoever is helping with Bloom");
		} catch (e) {
			setCopied(String(e));
		}
	};

	const connected = !!status?.connected;
	const details = status?.tool_details ?? [];

	return (
		<>
			<div className="setting-group-label">Account</div>
			<div className="setting-group">
				<SettingRow
					icon={Link2}
					label="Blitzit Account"
					desc={
						connecting
							? "Finish signing in in your browser…"
							: connected
								? "Connected through Blitzit's own connection for other apps. The sign-in is stored encrypted on this PC"
								: "Sign in once in your browser to add tasks from the notch"
					}
					divider={!!problem || connected}
				>
					{connecting ? (
						<button
							type="button"
							className="settings-button"
							onClick={() => void invoke("blitzit_cancel_connect")}
						>
							Cancel
						</button>
					) : connected ? (
						<button
							type="button"
							className="settings-button"
							onClick={() => void invoke("blitzit_disconnect").then(() => refresh())}
						>
							Disconnect
						</button>
					) : (
						<button type="button" className="settings-button" onClick={() => void connect()}>
							Connect
						</button>
					)}
				</SettingRow>
				{problem && (
					<SettingRow
						icon={TriangleAlert}
						label="Blitzit Said"
						desc={problem}
						divider={connected}
					/>
				)}
				{connected && (
					<SettingRow
						icon={RefreshCw}
						label="Check Again"
						desc={checking ? "Asking Blitzit…" : "Ask Blitzit what it offers now"}
						action={!checking}
						divider={false}
						onClick={checking ? undefined : () => void refresh()}
					/>
				)}
			</div>

			<div className="setting-group-label">In the Notch</div>
			<div className="setting-group">
				<SettingRow
					icon={PanelTop}
					label="Blitzit in Scroll Menu"
					desc="Scrolling or swiping on the notch also reaches Blitzit's page"
				>
					<label className="toggle-switch">
						<input
							type="checkbox"
							checked={inScroll}
							onChange={(e) => setInScroll(e.target.checked)}
						/>
						<span className="slider"></span>
					</label>
				</SettingRow>
				<SettingRow
					icon={Rocket}
					label="Blitz Now"
					desc={
						status && !status.app_found
							? "The Blitzit app wasn't found on this PC. Blitz now opens it once it's installed"
							: "Brings the Blitzit app forward and starts Blitz mode. Works without signing in"
					}
					divider={false}
				/>
			</div>

			{connected && (
				<>
					<div className="setting-group-label">Blitzit's Actions Bloom Uses</div>
					<div className="setting-group">
						{ACTIONS.map(({ id, label, icon }, index) => {
							const used = status?.using[index] ?? null;
							return (
								<SettingRow
									key={id}
									icon={icon}
									label={label}
									desc={
										used
											? `Uses "${used}"${choices[id] ? "" : ", found automatically"}`
											: details.length > 0
												? "Not found. Pick the one that does this"
												: "Not known until Blitzit answers"
									}
								>
									<select
										className="settings-select"
										value={choices[id] ?? ""}
										disabled={details.length === 0}
										onChange={(e) => choose(id, e.target.value)}
									>
										<option value="">Automatic</option>
										{details.map(([name, description]) => (
											<option key={name} value={name} title={description}>
												{name}
											</option>
										))}
									</select>
								</SettingRow>
							);
						})}
						<SettingRow
							icon={ClipboardCopy}
							label="Copy Blitzit's Actions"
							desc={copied ?? "Copies the full list with their inputs, to report a problem"}
							action
							divider={false}
							onClick={() => void copyReport()}
						/>
					</div>
				</>
			)}

			<div className="setting-group-label">Connection</div>
			<div className="setting-group">
				<SettingRow
					icon={Server}
					label="Blitzit Version"
					desc={
						connected
							? "Changing it needs Disconnect, then Connect again"
							: "Which Blitzit connection to sign in to"
					}
					divider={false}
				>
					<select
						className="settings-select"
						value={SERVERS.some((s) => s.url === server) ? server : SERVERS[0].url}
						onChange={(e) => setServer(e.target.value)}
					>
						{SERVERS.map((s) => (
							<option key={s.url} value={s.url}>
								{s.label}
							</option>
						))}
					</select>
				</SettingRow>
			</div>
		</>
	);
}
