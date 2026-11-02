import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { CircleCheck, LayoutGrid, ListX, Power, TriangleAlert } from "lucide-react";
import { SettingRow } from "./SettingRow";
import { useStoredFlag } from "./useStoredSetting";

/**
 * Settings > Integrations > Portals: Bloom starting Portals as administrator
 * after sign-in (through its own scheduled task, set up once with Windows'
 * permission), so Portals' loading pop-up stays out of sight.
 */

export interface PortalsStatus {
	installed: boolean;
	starts_itself: boolean;
	/** What went wrong setting it up, when something did. */
	task_problem: string | null;
	/** Bloom's task that starts Portals as administrator is set up. */
	task_ready: boolean;
	/** Other scheduled tasks that start Portals at sign-in, still on. */
	other_tasks: string[];
}

/** Portals' state and switching Bloom starting it on or off. */
export function usePortals() {
	const [status, setStatus] = useState<PortalsStatus | null>(null);
	const [on, setOn] = useStoredFlag("bloom-start-portals", false);
	const [settingUp, setSettingUp] = useState(false);

	const load = useCallback(
		() =>
			void invoke<PortalsStatus>("portals_status")
				.then(setStatus)
				.catch(() => {}),
		[]
	);

	useEffect(() => {
		load();
	}, [on, load]);

	/** Makes (or removes) Bloom's task; Windows asks for permission once. */
	const setUp = async (next: boolean) => {
		setSettingUp(true);
		try {
			await invoke("portals_set_up", { on: next });
		} catch {
			/* shown from the status */
		} finally {
			setSettingUp(false);
			load();
		}
	};

	const turn = (next: boolean) => {
		setOn(next);
		if (next && !status?.task_ready) void setUp(true);
		if (!next && status?.task_ready) void setUp(false);
	};

	return { status, on, settingUp, turn, setUp };
}

export function PortalsSettings() {
	const { status, on, settingUp, turn, setUp } = usePortals();

	if (status && !status.installed) {
		return (
			<>
				<div className="setting-group-label">Portals</div>
				<div className="setting-group">
					<SettingRow
						icon={LayoutGrid}
						label="Portals Wasn't Found"
						desc="Its settings show here once Portals is installed on this PC"
						divider={false}
					/>
				</div>
			</>
		);
	}

	const statusRow = (() => {
		if (!on || !status) return null;
		if (settingUp)
			return {
				icon: Power,
				label: "Setting Up",
				desc: "Waiting for Windows' permission…",
				button: null
			};
		if (status.task_problem)
			return {
				icon: TriangleAlert,
				label: "Couldn't Set It Up",
				desc: status.task_problem,
				button: { text: "Try again", run: () => void setUp(true) }
			};
		if (!status.task_ready)
			return {
				icon: TriangleAlert,
				label: "Not Set Up Yet",
				desc: "Portals would start without administrator rights until it is",
				button: { text: "Set up now", run: () => void setUp(true) }
			};
		return {
			icon: CircleCheck,
			label: "Set Up",
			desc: "Bloom starts Portals as administrator a few seconds after you sign in",
			button: null
		};
	})();

	const warnings = on && status && !settingUp && status.task_ready;

	return (
		<>
			<div className="setting-group-label">Startup</div>
			<div className="setting-group">
				<SettingRow
					icon={Power}
					label="Start Portals with Bloom"
					desc="As administrator, after you sign in, with its loading pop-up kept out of sight. Windows asks for permission once"
					divider={!!statusRow}
				>
					<label className="toggle-switch">
						<input
							type="checkbox"
							checked={on}
							disabled={settingUp}
							onChange={(e) => turn(e.target.checked)}
						/>
						<span className="slider"></span>
					</label>
				</SettingRow>
				{statusRow && (
					<SettingRow
						icon={statusRow.icon}
						label={statusRow.label}
						desc={statusRow.desc}
						divider={false}
					>
						{statusRow.button && (
							<button
								type="button"
								className="settings-button"
								disabled={settingUp}
								onClick={statusRow.button.run}
							>
								{statusRow.button.text}
							</button>
						)}
					</SettingRow>
				)}
			</div>

			{warnings && (status.other_tasks.length > 0 || status.starts_itself) && (
				<>
					<div className="setting-group-label">Also Starting Portals</div>
					<div className="setting-group">
						{status.other_tasks.length > 0 && (
							<SettingRow
								icon={ListX}
								label="Other Scheduled Tasks"
								desc={`Still on in Task Scheduler, starting Portals before Bloom: ${status.other_tasks.join(", ")}`}
								divider={status.starts_itself}
							>
								<button
									type="button"
									className="settings-button"
									disabled={settingUp}
									onClick={() => void setUp(true)}
								>
									Switch off
								</button>
							</SettingRow>
						)}
						{status.starts_itself && (
							<SettingRow
								icon={TriangleAlert}
								label="Portals Starts Itself"
								desc="Turn off Launch on Startup in Portals' App Settings, so Bloom starts it instead"
								divider={false}
							/>
						)}
					</div>
				</>
			)}

			<div className="setting-group-note">
				Your own scheduled task for Portals is switched off while this is on, and back on if you
				turn it off.
			</div>
		</>
	);
}
