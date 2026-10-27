import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ChevronRight, Gauge, Zap } from "lucide-react";
import type { BlitzitStatus } from "../components/BlitzitPanel";

/**
 * Settings > Integrations: the apps Bloom works with. Each one has its own
 * tab under Integrations in the sidebar, and its own page in the notch.
 */

export function IntegrationsTab({ openTab }: { openTab: (tab: string) => void }) {
	const [blitzit, setBlitzit] = useState<BlitzitStatus | null>(null);

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
						<Gauge size={14} strokeWidth={1.5} />
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
		</>
	);
}
