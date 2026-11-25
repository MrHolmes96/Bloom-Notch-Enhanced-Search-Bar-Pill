import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

/**
 * Notes a page's slow frames in `perf.log` (Bloom's diagnostics): how many
 * took 120 ms or more in the last 10 s, the longest, and the code that ran
 * in it, when the web view can say (long animation frames). Costs nothing
 * while frames are quick: the web view reports only the slow ones.
 */
export function usePerfWatch(page: string) {
	useEffect(() => {
		const kinds = PerformanceObserver.supportedEntryTypes ?? [];
		const type = kinds.includes("long-animation-frame") ? "long-animation-frame" : kinds.includes("longtask") ? "longtask" : null;
		if (!type) return;
		let count = 0;
		let longest = 0;
		let blame = "";
		const observer = new PerformanceObserver((list) => {
			for (const entry of list.getEntries()) {
				if (entry.duration < 120) continue;
				count += 1;
				if (entry.duration > longest) {
					longest = entry.duration;
					// The script that took longest in that frame, where it came from.
					const scripts = ((entry as any).scripts ?? []) as any[];
					const top = scripts.sort((a, b) => b.duration - a.duration)[0];
					blame = top
						? `${top.invoker || top.invokerType || "script"} ${Math.round(top.duration)} ms` +
							(top.sourceFunctionName ? ` in ${top.sourceFunctionName}` : "") +
							(top.sourceURL ? ` (${String(top.sourceURL).split("/").pop()}:${top.sourceCharPosition ?? ""})` : "")
						: (entry as any).renderStart
							? `drawing ${Math.round(entry.startTime + entry.duration - (entry as any).renderStart)} ms`
							: "";
				}
			}
		});
		try {
			observer.observe({ type, buffered: false });
		} catch {
			return;
		}
		const timer = window.setInterval(() => {
			if (!count) return;
			const what = `${count} slow frame(s), longest ${Math.round(longest)} ms${blame ? `: ${blame}` : ""}`;
			count = 0;
			longest = 0;
			blame = "";
			void invoke("perf_note", { page, what }).catch(() => {});
		}, 10_000);
		return () => {
			observer.disconnect();
			window.clearInterval(timer);
		};
	}, [page]);
}
