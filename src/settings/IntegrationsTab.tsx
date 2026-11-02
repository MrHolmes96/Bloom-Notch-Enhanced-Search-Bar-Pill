import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ChevronRight, LayoutGrid, Zap } from "lucide-react";
import { useStoredFlag } from "./useStoredSetting";
import { GHelperIcon } from "../icons/GHelperIcon";
import type { BlitzitStatus } from "../components/BlitzitPanel";

/**
 * Settings > Integrations: the apps Bloom works with. Each one has its own
 * tab under Integrations in the sidebar, and its own page in the notch.
 */

interface PortalsStatus {
	installed: boolean;
	starts_itself: boolean;
	/** What went wrong setting it up, when something did. */
	task_problem: string | null;
	/** Bloom's task that starts Portals as administrator is set up. */
	task_ready: boolean;
	/** Other scheduled tasks that start Portals at sign-in, still on. */
	other_tasks: string[];
}

export function IntegrationsTab({ openTab }: { openTab: (tab: string) => void }) {
	const [blitzit, setBlitzit] = useState<BlitzitStatus | null>(null);
	const [portals, setPortals] = useState<PortalsStatus | null>(null);
	const [startPortals, setStartPortals] = useStoredFlag("bloom-start-portals", false);
	const [settingUp, setSettingUp] = useState(false);

	const loadPortals = () =>
		void invoke<PortalsStatus>("portals_status")
			.then(setPortals)
			.catch(() => {});

	useEffect(() => {
		loadPortals();
	}, [startPortals]);

	/** Makes (or removes) Bloom's task; Windows asks for permission once. */
	const setUpPortals = async (on: boolean) => {
		setSettingUp(true);
		try {
			await invoke("portals_set_up", { on });
		} catch {
			/* shown from the status */
		} finally {
			setSettingUp(false);
			loadPortals();
		}
	};

	const toggleStartPortals = () => {
		const next = !startPortals;
		setStartPortals(next);
		if (next && !portals?.task_ready) void setUpPortals(true);
		if (!next && portals?.task_ready) void setUpPortals(false);
	};

	useEffect(() => {
		const load = () =>
			void invoke<BlitzitStatus>("blitzit_status", { check: false })
				.then(setBlitzit)
				.catch(() => {});
		load();
		const changed = listen("blitzit-changed", load);
		return () => {
			void changed.then((f) => f());
		};
	}, []);

	return (
		<>
			<div className="setting-group-label">Apps</div>
			<div className="setting-group">
				<div className="setting-item action" onClick={() => openTab("integrations/blitzit")}>
					<div className="setting-icon-bg">
						<Zap size={14} strokeWidth={1.5} />
					</div>
					<div className="setting-info">
						<span className="setting-label">Blitzit</span>
						<span className="setting-desc">
							{blitzit?.connected
								? "Connected: add tasks, tick them off and start Blitz mode from the notch"
								: "Add tasks and start Blitz mode from the notch"}
						</span>
					</div>
					<ChevronRight size={14} strokeWidth={1.5} />
				</div>
				<div className="setting-divider" />
				<div className="setting-item action" onClick={() => openTab("integrations/ghelper")}>
					<div className="setting-icon-bg">
						<GHelperIcon size={14} strokeWidth={1.5} />
					</div>
					<div className="setting-info">
						<span className="setting-label">G-Helper</span>
						<span className="setting-desc">
							Modes, GPU mode, screen and charge limit in the notch, G-Helper's key bindings,
							and shortcuts for all of them (ASUS laptops with G-Helper)
						</span>
					</div>
					<ChevronRight size={14} strokeWidth={1.5} />
				</div>
			</div>
			<div className="setting-group-note">
				Each app gets its own page under Integrations, and its own page in the notch.
			</div>
			{portals?.installed && (
				<>
					<div className="setting-group-label">Portals</div>
					<div className="setting-group">
						<div className="setting-item">
							<div className="setting-icon-bg">
								<LayoutGrid size={14} strokeWidth={1.5} />
							</div>
							<div className="setting-info">
								<span className="setting-label">Start Portals with Bloom</span>
								<span className="setting-desc">
									Bloom starts Portals as administrator a few seconds after you sign in, so its
									loading pop-up can be kept out of sight. Windows asks for permission once, to
									set this up; your own scheduled task for Portals is switched off meanwhile (and
									back on if you turn this off)
								</span>
							</div>
							<label className="toggle-switch">
								<input
									type="checkbox"
									checked={startPortals}
									disabled={settingUp}
									onChange={toggleStartPortals}
								/>
								<span className="slider"></span>
							</label>
						</div>
					</div>
					{startPortals && portals.task_ready && portals.other_tasks.length === 0 && (
						<div className="setting-group-note">
							Set up: Bloom starts Portals as administrator after you sign in.
						</div>
					)}
					{startPortals && !portals.task_ready && (
						<div className="setting-group-note">
							Not set up yet, so Portals would start without administrator rights.{" "}
							<button
								type="button"
								className="settings-button"
								disabled={settingUp}
								onClick={() => void setUpPortals(true)}
							>
								{settingUp ? "Waiting for Windows…" : "Set up now"}
							</button>
						</div>
					)}
					{startPortals && portals.task_ready && portals.other_tasks.length > 0 && (
						<div className="setting-group-note">
							Still on in Task Scheduler, starting Portals before Bloom: {portals.other_tasks.join(", ")}.{" "}
							<button
								type="button"
								className="settings-button"
								disabled={settingUp}
								onClick={() => void setUpPortals(true)}
							>
								Switch off
							</button>
						</div>
					)}
					{startPortals && portals.task_problem && (
						<div className="setting-group-note">{portals.task_problem}.</div>
					)}
					{startPortals && portals.starts_itself && (
						<div className="setting-group-note">
							Portals still starts itself: turn off Launch on Startup in Portals' App Settings, so
							Bloom starts it instead.
						</div>
					)}
				</>
			)}
		</>
	);
}
