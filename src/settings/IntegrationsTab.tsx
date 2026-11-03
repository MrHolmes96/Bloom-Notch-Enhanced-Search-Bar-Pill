import { useEffect, useState, type ComponentType, type SVGProps } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ChevronRight, LayoutGrid, X, Zap } from "lucide-react";
import { useStoredFlag } from "./useStoredSetting";
import { usePortals } from "./PortalsSettings";
import { GHelperIcon } from "../icons/GHelperIcon";
import type { BlitzitStatus } from "../components/BlitzitPanel";
import type { GHelperState } from "../components/GHelperPanel";

/**
 * Settings > Integrations: the apps Bloom works with, each connected or not
 * from here. Clicking one opens its own page (also under Integrations in the
 * sidebar).
 */

type Icon = ComponentType<SVGProps<SVGSVGElement> & { size?: number; strokeWidth?: number }>;

type Connection =
	| { kind: "connected"; disconnect: () => void }
	| { kind: "connect"; connect: () => void }
	| { kind: "busy"; text: string; cancel?: () => void }
	| { kind: "missing"; text: string }
	| { kind: "loading" };

function AppRow({
	icon: AppIcon,
	name,
	desc,
	connection,
	open,
	last
}: {
	icon: Icon;
	name: string;
	desc: string;
	connection: Connection;
	open: () => void;
	last?: boolean;
}) {
	const stop = (e: React.MouseEvent) => e.stopPropagation();
	return (
		<>
			<div className="setting-item action" onClick={open}>
				<div className="setting-icon-bg">
					<AppIcon size={14} strokeWidth={1.5} />
				</div>
				<div className="setting-info">
					<span className="setting-label">{name}</span>
					<span className="setting-desc">{desc}</span>
				</div>
				<div className="int-actions" onClick={stop}>
					{connection.kind === "connected" && (
						<>
							<span className="int-connected">Connected</span>
							<button
								type="button"
								className="int-disconnect"
								title={`Disconnect ${name}`}
								aria-label={`Disconnect ${name}`}
								onClick={connection.disconnect}
							>
								<X size={12} strokeWidth={2} />
							</button>
						</>
					)}
					{connection.kind === "connect" && (
						<button
							type="button"
							className="settings-button int-connect"
							onClick={connection.connect}
						>
							Connect
						</button>
					)}
					{connection.kind === "busy" && (
						<>
							<span className="int-busy">{connection.text}</span>
							{connection.cancel && (
								<button type="button" className="settings-button" onClick={connection.cancel}>
									Cancel
								</button>
							)}
						</>
					)}
					{connection.kind === "missing" && <span className="int-busy">{connection.text}</span>}
				</div>
				<ChevronRight size={14} strokeWidth={1.5} className="int-chevron" />
			</div>
			{!last && <div className="setting-divider" />}
		</>
	);
}

export function IntegrationsTab({ openTab }: { openTab: (tab: string) => void }) {
	const [blitzit, setBlitzit] = useState<BlitzitStatus | null>(null);
	const [signingIn, setSigningIn] = useState(false);
	const [ghelper, setGhelper] = useState<GHelperState | null>(null);
	const [ghelperOn, setGhelperOn] = useStoredFlag("bloom-ghelper-enabled", true);
	const portals = usePortals();

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

	useEffect(() => {
		const load = () =>
			void invoke<GHelperState>("ghelper_state", { sensors: false })
				.then(setGhelper)
				.catch(() => {});
		load();
		const changed = listen("ghelper-changed", load);
		return () => {
			void changed.then((f) => f());
		};
	}, []);

	const connectBlitzit = async () => {
		setSigningIn(true);
		try {
			setBlitzit(await invoke<BlitzitStatus>("blitzit_connect"));
		} catch {
			/* the Blitzit page shows what went wrong */
		} finally {
			setSigningIn(false);
		}
	};

	const blitzitConnection: Connection = signingIn
		? {
				kind: "busy",
				text: "Sign in in your browser…",
				cancel: () => void invoke("blitzit_cancel_connect")
			}
		: !blitzit
			? { kind: "loading" }
			: blitzit.connected
				? {
						kind: "connected",
						disconnect: () =>
							void invoke("blitzit_disconnect")
								.then(() => invoke<BlitzitStatus>("blitzit_status", { check: false }))
								.then(setBlitzit)
								.catch(() => {})
					}
				: { kind: "connect", connect: () => void connectBlitzit() };

	const ghelperConnection: Connection = !ghelper
		? { kind: "loading" }
		: !ghelper.installed
			? { kind: "missing", text: "Not found on this PC" }
			: ghelperOn
				? { kind: "connected", disconnect: () => setGhelperOn(false) }
				: { kind: "connect", connect: () => setGhelperOn(true) };

	const portalsConnection: Connection = !portals.status
		? { kind: "loading" }
		: !portals.status.installed
			? { kind: "missing", text: "Not found on this PC" }
			: portals.settingUp
				? { kind: "busy", text: "Waiting for Windows…" }
				: portals.on
					? { kind: "connected", disconnect: () => portals.turn(false) }
					: { kind: "connect", connect: () => portals.turn(true) };

	return (
		<>
			<div className="setting-group-label">Apps</div>
			<div className="setting-group">
				<AppRow
					icon={Zap}
					name="Blitzit"
					desc="Your tasks and Blitz mode in the notch, synced with your Blitzit account"
					connection={blitzitConnection}
					open={() => openTab("integrations/blitzit")}
				/>
				<AppRow
					icon={GHelperIcon}
					name="G-Helper"
					desc="Modes, GPU mode, screen and charge limit in the notch, and G-Helper's key bindings"
					connection={ghelperConnection}
					open={() => openTab("integrations/ghelper")}
				/>
				<AppRow
					icon={LayoutGrid}
					name="Portals"
					desc="Starts as administrator as you sign in, its loading pop-up out of sight"
					connection={portalsConnection}
					open={() => openTab("integrations/portals")}
					last
				/>
			</div>
			<div className="setting-group-note">Click an app for its settings.</div>
		</>
	);
}
