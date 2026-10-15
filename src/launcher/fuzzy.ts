import type { Precision } from "./config";

/**
 * How well `name` matches `query`, 0 for no match, up to 1000 for an exact
 * match. Prefixes beat word starts beat initials beat substrings beat
 * letters-in-order.
 */
export function scoreMatch(query: string, name: string): number {
	const q = query.trim().toLowerCase();
	const n = name.toLowerCase();
	if (!q || !n) return 0;
	if (n === q) return 1000;
	if (n.startsWith(q)) return 900 - Math.min(n.length - q.length, 100);

	const words = n.split(/[\s\-_.()[\]/\\,:+]+/).filter(Boolean);
	const queryWords = q.split(/\s+/).filter(Boolean);

	// "vs code" finds "Visual Studio Code": every typed word starts a word.
	if (queryWords.length > 1) {
		let from = 0;
		let inOrder = true;
		const used = new Set<number>();
		for (const qw of queryWords) {
			const at = words.findIndex((w, i) => !used.has(i) && w.startsWith(qw));
			if (at < 0) {
				inOrder = false;
				break;
			}
			used.add(at);
			if (at < from) inOrder = false;
			from = at;
		}
		if (used.size === queryWords.length) return inOrder ? 780 : 720;
	}

	const wordIndex = words.findIndex((w) => w.startsWith(q));
	if (wordIndex >= 0) return 700 - Math.min(wordIndex * 10, 50);

	// Initials: "vsc" finds "Visual Studio Code".
	const initials = words.map((w) => w[0]).join("");
	if (q.length > 1 && !q.includes(" ") && initials.startsWith(q)) return 650;

	const at = n.indexOf(q);
	if (at >= 0) return 500 - Math.min(at, 100);

	if (queryWords.length > 1 && queryWords.every((qw) => n.includes(qw))) return 400;

	// Letters in order with gaps: "spfy" finds "Spotify".
	const compact = q.replace(/\s+/g, "");
	if (compact.length < 2) return 0;
	let qi = 0;
	let gaps = 0;
	let last = -1;
	for (let i = 0; i < n.length && qi < compact.length; i++) {
		if (n[i] === compact[qi]) {
			if (last >= 0) gaps += i - last - 1;
			last = i;
			qi++;
		}
	}
	if (qi === compact.length) return Math.max(1, 300 - gaps * 10);
	return 0;
}

/** The lowest score a result needs, by the "Search precision" setting. */
export function minScore(precision: Precision): number {
	switch (precision) {
		case "none":
			return 1;
		case "low":
			return 80;
		default:
			return 180;
	}
}

/** The best score of a name against several of its search words. */
export function bestMatch(
	query: string,
	names: (string | undefined)[],
	weights?: number[]
): number {
	let best = 0;
	names.forEach((name, i) => {
		if (!name) return;
		const s = scoreMatch(query, name) * (weights?.[i] ?? 1);
		if (s > best) best = s;
	});
	return best;
}
