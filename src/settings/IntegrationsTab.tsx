import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Link2, ListChecks, PanelTop, Rocket, Server, Zap } from "lucide-react";
import { SettingRow } from "./SettingRow";
import { useStoredFlag, useStoredString } from "./useStoredSetting";
import type { BlitzitStatus } from "../components/BlitzitPanel";

/**
 * Settings > Integrations: other apps in the notch. Each app gets its own
 * group here and its own page in the notch's scroll menu.
 */

const SERVERS = [
	{ url: "https://core.blitzit.app/v3/mcp", label: "Blitzit 3 and newer" },
	{ url: "https://integration-prd.blitzit.app/mcp", label: "Older Blitzit" }
];

function capabilities(status: BlitzitStatus) {
	const can: string[] = [];
	if (status.can_add) can.push("add tasks");
	if (status.can_read_lists) can.push("show your lists");
	if (status.can_read_tasks) can.push("show today's tasks");
	if (status.can_complete) can.push("tick tasks off");
	if (can.length === 0) {
		return status.tools.length > 0
			? `Blitzit offers ${status.tools.join(", ")}, none of which Bloom uses yet`
			: "Checking what Blitzit shares…";
	}
	const text = can.join(", ").replace(/, ([^,]*)$/, " and $1");
	return `The notch can ${text}`;
}

export function IntegrationsTab() {
	const [status, setStatus] = useState<BlitzitStatus | null>(null);
	const [connecting, setConnecting] = useState(false);
	const [problem, setProblem] = useState<string | null>(null);
	const [inScroll, setInScroll] = useStoredFlag("bloom-notch-blitzit", true);
	const [server, setServer] = useStoredString("bloom-blitzit-server", SERVERS[0].url);

	const refresh = useCallback(async (check: boolean) => {
		try {
			setStatus(await invoke<BlitzitStatus>("blitzit_status", { check }));
		} catch (e) {
			setProblem(String(e));
		}
	}, []);

	useEffect(() => {
		void refresh(true);
		const changed = listen("blitzit-changed", () => void refresh(false));
		return () => {
			void changed.then((f) => f());
		};
	}, [refresh]);

	const connect = async () => {
		setConnecting(true);
		setProblem(null);
		try {
			setStatus(await invoke<BlitzitStatus>("blitzit_connect"));
		} catch (e) {
			setProblem(String(e));
		} finally {
			setConnecting(false);
		}
	};

	const connected = !!status?.connected;

	return (
		<>
			<div className="setting-group-label">Blitzit</div>
			<div className="setting-group">
				<SettingRow
					icon={Link2}
					label="Blitzit Account"
					desc={
						problem
							? problem
							: connecting
								? "Finish signing in in your browser…"
								: connected
									? "Connected through Blitzit's own app connection. The sign-in is stored encrypted on this PC"
									: "Sign in once in your browser to add tasks from the notch"
					}
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
							onClick={() => {
								void invoke("blitzit_disconnect").then(() => refresh(false));
							}}
						>
							Disconnect
						</button>
					) : (
						<button type="button" className="settings-button" onClick={() => void connect()}>
							Connect
						</button>
					)}
				</SettingRow>
				{connected && status && (
					<SettingRow icon={ListChecks} label="What Works" desc={capabilities(status)} />
				)}
				<SettingRow
					icon={Rocket}
					label="Blitz Mode"
					desc={
						status && !status.app_found
							? "The Blitzit app wasn't found on this PC. Blitz now opens it once it's installed"
							: "Blitz now in the notch brings Blitzit forward and starts Blitz mode. No sign-in needed"
					}
				/>
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
			<div className="setting-group-note">
				<Zap size={11} /> More apps, such as Obsidian, can get their own page in the notch the same
				way later.
			</div>
		</>
	);
}
