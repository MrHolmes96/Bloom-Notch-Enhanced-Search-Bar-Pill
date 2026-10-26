import { useState } from "react";
import {
	Download,
	FileDown,
	Upload,
	FolderOpen,
	HardDrive,
	Wrench,
	FileArchive,
	LifeBuoy
} from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { GithubIcon } from "../icons";
import { SettingRow } from "./SettingRow";

/** Where this build's new versions are published. */
const BUILDS_URL =
	"https://github.com/MrHolmes96/bloom-notch-enhanced-search-bar-pill/releases/tag/personal-build";

interface AboutTabProps {
	appVersion: string;
	exportStatus: string;
	importStatus: string;
	handleExportSettings: () => void;
	handleImportSettings: () => void;
}

export function AboutTab({
	appVersion,
	exportStatus,
	importStatus,
	handleExportSettings,
	handleImportSettings
}: AboutTabProps) {
	const [folderError, setFolderError] = useState("");
	const [repairNote, setRepairNote] = useState<string | null>(null);
	const [diagnosticsNote, setDiagnosticsNote] = useState<string | null>(null);
	const repair = () => {
		setRepairNote("Repairing…");
		invoke<string[]>("bloom_repair")
			.then((done) => setRepairNote(done.join(". ")))
			.catch((e) => setRepairNote(String(e)));
	};
	const exportDiagnostics = () => {
		setDiagnosticsNote("Saving…");
		invoke<string>("diagnostics_export")
			.then((path) => setDiagnosticsNote(`Saved on your desktop: ${path.split("\\").pop()}`))
			.catch((e) => setDiagnosticsNote(String(e)));
	};
	const openFolder = (kind: "data" | "cache") => {
		setFolderError("");
		invoke("open_bloom_folder", { kind }).catch((e) => setFolderError(String(e)));
	};

	const getExportLabel = () => {
		if (exportStatus === "exporting") return "Exporting...";
		if (exportStatus === "success") return "Exported!";
		return "Export Backup";
	};

	const getImportLabel = () => {
		if (importStatus === "importing") return "Importing...";
		if (importStatus === "success") return "Imported!";
		return "Import Backup";
	};

	return (
		<div className="about-tab-container">
			<div className="about-header">
				<img src="/bloom.png" className="about-logo" alt="Bloom Logo" draggable={false} />
				<h1 className="about-title">Bloom</h1>
				<p className="about-version">Version {appVersion}</p>
				<p className="about-credit">
					Architected and Curated by{" "}
					<button
						className="about-author"
						onClick={() => openUrl("https://github.com/MrHolmes96")}
						title="MrHolmes on GitHub"
					>
						<GithubIcon size={12} />
						<span>MrHolmes</span>
					</button>
				</p>
				<p className="about-credit about-credit--foundation">
					Built on the Bloom foundation by{" "}
					<button
						className="about-author"
						onClick={() => openUrl("https://github.com/SehajveerSingh2005")}
						title="sehaz on GitHub"
					>
						<GithubIcon size={12} />
						<span>sehaz</span>
					</button>
				</p>
			</div>

			<div className="setting-group-label">Updates</div>
			<div className="setting-group">
				<SettingRow
					icon={Download}
					label="Get New Builds"
					desc={`Running v${appVersion}. Bloom never looks for updates itself; new builds are on your personal-build release`}
					action
					divider={false}
					onClick={() => openUrl(BUILDS_URL)}
				/>
			</div>

			<div className="setting-group-label setting-group-label--spaced">Data</div>
			<div className="setting-group">
				<SettingRow
					icon={FolderOpen}
					label="Open Data Folder"
					desc={folderError || "Settings, logs and tray-icons.txt"}
					action
					onClick={() => openFolder("data")}
				/>
				<SettingRow
					icon={HardDrive}
					label="Open Cache Folder"
					desc="The notch's web storage and caches"
					action
					onClick={() => openFolder("cache")}
				/>
				<SettingRow
					icon={FileDown}
					label={getExportLabel()}
					desc="Every setting, pinned dock apps, custom icons, window sizes and search history, in one file"
					action
					onClick={handleExportSettings}
				/>
				<SettingRow
					icon={Upload}
					label={getImportLabel()}
					desc="Restores a backup (dock apps not on this PC are left out), then Bloom restarts"
					action
					divider={false}
					onClick={handleImportSettings}
				/>
			</div>
			<div className="setting-group-label setting-group-label--spaced">Recovery & Diagnostics</div>
			<div className="setting-group">
				<SettingRow
					icon={Wrench}
					label="Repair Bloom"
					desc={
						repairNote ??
						"Puts back what Bloom needs (its startup, Explorer as Windows' shell). Done by itself after Windows updates"
					}
					action
					onClick={repair}
				/>
				<SettingRow
					icon={FileArchive}
					label="Export Diagnostics"
					desc={
						diagnosticsNote ??
						"Bloom's logs and a health report in a zip on your desktop (no settings, sign-ins or notification texts)"
					}
					action
					onClick={exportDiagnostics}
				/>
				<SettingRow
					icon={LifeBuoy}
					label="Bloom Recovery"
					desc='If Bloom ever leaves you without a taskbar: press the Windows key and type "Bloom Recovery" (or Ctrl+Shift+Esc > Run new task > "Bloom.exe --recovery"). Windows is put back as it was and Bloom stops starting with Windows'
					divider={false}
				/>
			</div>

			<div className="setting-group-note">
				Everything Bloom keeps stays in these two folders on this PC, and nothing is sent anywhere
				on its own. Bloom only goes online, always encrypted, for the weather (Open-Meteo, for the
				city you pick; it never looks up where you are), search suggestions if you turn them on,
				and Blitzit if you connect it. LocalSend stays on your home network.
			</div>
		</div>
	);
}
