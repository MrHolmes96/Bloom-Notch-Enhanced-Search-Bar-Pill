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
	/** A scheduled task starting Portals that Bloom couldn't switch off. */
	task_problem: string | null;
}

export function IntegrationsTab({ openTab }: { openTab: (tab: string) => void }) {
	const [blitzit, setBlitzit] = useState<BlitzitStatus | null>(null);
	const [portals, setPortals] = useState<PortalsStatus | null>(null);
	const [startPortals, setStartPortals] = useStoredFlag("bloom-start-portals", false);

	useEffect(() => {
		const load = () =>
			void invoke<PortalsStatus>("portals_status")
				.then(setPortals)
				.catch(() => {});
		load();
		// Switching Portals' scheduled tasks takes a moment: check again.
		const timer = window.setTimeout(load, 1500);
		return () => window.clearTimeout(timer);
	}, [startPortals]);

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
									Bloom starts Portals a few seconds after you sign in, so its loading pop-up can
									be kept out of sight. A scheduled task that starts Portals is switched off
									meanwhile (and back on if you turn this off)
								</span>
							</div>
							<label className="toggle-switch">
								<input
									type="checkbox"
									checked={startPortals}
									onChange={() => setStartPortals(!startPortals)}
								/>
								<span className="slider"></span>
							</label>
						</div>
					</div>
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
