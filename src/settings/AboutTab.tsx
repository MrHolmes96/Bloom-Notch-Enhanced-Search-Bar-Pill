import { useState } from "react";
import { Download, FileDown, Upload, FolderOpen, HardDrive } from "lucide-react";
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
	const openFolder = (kind: "data" | "cache") => {
		setFolderError("");
		invoke("open_bloom_folder", { kind }).catch((e) => setFolderError(String(e)));
	};

	const getExportLabel = () => {
		if (exportStatus === "exporting") return "Exporting...";
		if (exportStatus === "success") return "Exported!";
		return "Export Settings";
	};

	const getImportLabel = () => {
		if (importStatus === "importing") return "Importing...";
		if (importStatus === "success") return "Imported!";
		return "Import Settings";
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
					desc="Save settings to a file"
					action
					onClick={handleExportSettings}
				/>
				<SettingRow
					icon={Upload}
					label={getImportLabel()}
					desc="Load settings from a file"
					action
					divider={false}
					onClick={handleImportSettings}
				/>
			</div>
			<div className="setting-group-note">
				Everything Bloom keeps stays in these two folders on this PC, and nothing is sent anywhere
				on its own. Bloom only goes online for the weather (Open-Meteo, for the city you set; with
				no city set, your rough location is looked up from your IP address), search suggestions if
				you turn them on, Blitzit if you connect it, and LocalSend, which stays on your home
				network.
			</div>
		</div>
	);
}
