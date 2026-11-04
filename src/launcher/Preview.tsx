import { useEffect, useState } from "react";
import { formatClock, useClock24h } from "../hooks/useClock24h";
import { invoke } from "@tauri-apps/api/core";
import { ResultIcon } from "./icons";
import type { Result } from "./types";

interface FileInfo {
	path: string;
	is_dir: boolean;
	size: number | null;
	modified: number | null;
	created: number | null;
}

function formatSize(bytes: number) {
	const units = ["B", "KB", "MB", "GB", "TB"];
	let value = bytes;
	let unit = 0;
	while (value >= 1024 && unit < units.length - 1) {
		value /= 1024;
		unit++;
	}
	return `${value.toFixed(unit === 0 ? 0 : 1)} ${units[unit]}`;
}

/** yyyy-MM-dd and the time as Bloom's clock is set (Settings > General). */
function formatDate(ms: number, h24: boolean) {
	const d = new Date(ms);
	const pad = (n: number) => String(n).padStart(2, "0");
	const date = `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
	return `${date} ${formatClock(d, h24)}`;
}

/** The F1 panel: a picture of the file and its size and dates. */
export function Preview({
	preview,
	title
}: {
	preview: NonNullable<Result["preview"]>;
	title: string;
}) {
	const [thumb, setThumb] = useState<string | null>(null);
	const [info, setInfo] = useState<FileInfo | null>(null);
	const h24 = useClock24h();

	useEffect(() => {
		let alive = true;
		setThumb(null);
		setInfo(null);
		if (preview.kind !== "app") {
			void invoke<string | null>("launcher_thumbnail", { path: preview.path }).then((t) => {
				if (alive) setThumb(t);
			});
			void invoke<FileInfo | null>("launcher_file_info", { path: preview.path }).then((i) => {
				if (alive) setInfo(i);
			});
		}
		return () => {
			alive = false;
		};
	}, [preview.kind, preview.path]);

	return (
		<div className="launcher-preview">
			<div className="launcher-preview-image">
				{thumb ? (
					<img src={thumb} alt="" draggable={false} />
				) : (
					<ResultIcon
						icon={
							preview.kind === "app"
								? { kind: "app", path: preview.path, name: title }
								: { kind: "file", path: preview.path }
						}
						size={96}
					/>
				)}
			</div>
			<div className="launcher-preview-title">{title}</div>
			<div className="launcher-preview-path">{preview.path}</div>
			{info && (
				<dl className="launcher-preview-facts">
					{info.size !== null && (
						<>
							<dt>Size</dt>
							<dd>{formatSize(info.size)}</dd>
						</>
					)}
					{info.modified && (
						<>
							<dt>Modified</dt>
							<dd>{formatDate(info.modified, h24)}</dd>
						</>
					)}
					{info.created && (
						<>
							<dt>Created</dt>
							<dd>{formatDate(info.created, h24)}</dd>
						</>
					)}
				</dl>
			)}
		</div>
	);
}
