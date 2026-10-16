import { invoke } from "@tauri-apps/api/core";
import { bestMatch } from "../fuzzy";
import { copy, open, webUrl } from "../ops";
import { allWebSources, type WebSource } from "../config";
import type { Result, SearchContext } from "../types";

function webResult(
	ctx: SearchContext,
	source: WebSource,
	text: string,
	score: number,
	suggestion = false
): Result {
	const url = webUrl(source.url, text);
	return {
		key: `web:${source.id}:${suggestion ? "s:" : ""}${text.toLowerCase()}`,
		title: text || `Search ${source.title}`,
		subtitle: `Search ${source.title}`,
		icon: { kind: "glyph", glyph: suggestion ? "search" : "globe" },
		score,
		plugin: "web",
		noHistory: !text,
		keepOpen: !text,
		run: () => {
			if (!text) {
				if (source.keyword !== "*") ctx.setQuery(`${source.keyword} `);
				return;
			}
			return open(url);
		},
		copyText: url,
		actions: () => [
			{ key: "open", title: "Open in browser", glyph: "external", run: () => open(url) },
			{ key: "copy-url", title: "Copy URL", glyph: "copy", run: () => copy(url) }
		],
		replay: text ? { type: "url", url, title: `${text} – ${source.title}` } : undefined
	};
}

/**
 * Web searches. With a keyword ("g cats") only that site; without one the
 * sites set to "*" add a "search the web" row at the bottom.
 */
export function searchWeb(ctx: SearchContext, keyword: string | null, search: string): Result[] {
	const text = search.trim();
	const sources = allWebSources(ctx.config).filter(
		(s) => s.enabled && (keyword ? s.keyword === keyword : s.keyword === "*")
	);
	if (!keyword && !text) return [];
	return sources.map((s) => webResult(ctx, s, text, keyword ? 1450 : 50));
}

/** Suggestions from the chosen engine, under the web search rows. */
export async function webSuggestions(
	ctx: SearchContext,
	keyword: string | null,
	search: string,
	isCurrent: () => boolean
): Promise<Result[]> {
	const { web } = ctx.config;
	const text = search.trim();
	if (!web.suggestions || !text) return [];
	const sources = allWebSources(ctx.config).filter(
		(s) => s.enabled && (keyword ? s.keyword === keyword : s.keyword === "*")
	);
	if (!sources.length) return [];
	const suggestions = await invoke<string[]>("launcher_web_suggestions", {
		source: web.suggestionSource,
		query: text
	}).catch(() => [] as string[]);
	if (!isCurrent()) return [];
	const out: Result[] = [];
	for (const source of sources) {
		suggestions
			.filter((s) => s.toLowerCase() !== text.toLowerCase())
			.slice(0, web.maxSuggestions)
			.forEach((s, i) =>
				out.push(webResult(ctx, source, s, (keyword ? 1440 : 48) - i * 0.1, true))
			);
	}
	return out;
}

const COMMON_TLDS = new Set(
	"com org net io dev app gov edu co ai gg me tv info biz xyz eu uk de dk se no fi is fr nl be ch at it es pt pl cz ru ua jp cn kr in br mx ca us au nz za ly to sh cc page site online store tech blog news wiki".split(
		" "
	)
);

/** A typed web address: "example.com/page", "https://...", "localhost:3000". */
export function toUrl(text: string, preferHttps: boolean): { url: string; sure: boolean } | null {
	const t = text.trim();
	if (!t || /\s/.test(t)) return null;
	if (/^(https?|ftp):\/\/[^\s/$.?#].\S*$/i.test(t)) return { url: t, sure: true };
	if (/^(chrome|edge|brave|about|file):\/\/\S+$/i.test(t) || /^(about|mailto):\S+$/i.test(t)) {
		return { url: t, sure: true };
	}
	const scheme = preferHttps ? "https://" : "http://";
	const hostPart = t.split(/[/?#]/)[0];
	const host = hostPart.replace(/:\d{1,5}$/, "");
	if (/^localhost$/i.test(host)) return { url: `http://${t}`, sure: true };
	if (/^(\d{1,3}\.){3}\d{1,3}$/.test(host)) return { url: `http://${t}`, sure: true };
	// A plain number such as 3.14159 is not an address.
	if (/^[\d.,]+$/.test(t)) return null;
	const parts = host.split(".");
	if (parts.length < 2 || parts.some((p) => !/^[a-z0-9]([a-z0-9-]*[a-z0-9])?$/i.test(p)))
		return null;
	const tld = parts[parts.length - 1].toLowerCase();
	if (tld.length < 2 || !/^[a-z0-9]+$/.test(tld)) return null;
	// "notepad.exe" is a file name more often than a site.
	const sure = COMMON_TLDS.has(tld) || /^www\./i.test(host);
	return { url: `${scheme}${t}`, sure };
}

export function searchUrl(ctx: SearchContext, search: string): Result[] {
	const found = toUrl(search, ctx.config.url.https);
	if (!found) return [];
	const { url } = found;
	return [
		{
			key: `url:${url.toLowerCase()}`,
			title: url,
			subtitle: "Open this address in your browser",
			icon: { kind: "glyph", glyph: "link" },
			score: found.sure ? 1400 : 120,
			plugin: "url",
			run: () => open(url),
			copyText: url,
			actions: () => [
				{ key: "open", title: "Open in browser", glyph: "external", run: () => open(url) },
				{ key: "copy", title: "Copy URL", glyph: "copy", run: () => copy(url) }
			],
			replay: { type: "url", url, title: url }
		}
	];
}

export interface Bookmark {
	name: string;
	url: string;
	browser: string;
	folder: string;
}

let bookmarkCache: { at: number; list: Bookmark[]; browsers: string } | null = null;

export function clearBookmarkCache() {
	bookmarkCache = null;
}

async function bookmarks(browsers: string[]): Promise<Bookmark[]> {
	const id = browsers.join(",");
	if (bookmarkCache && bookmarkCache.browsers === id && Date.now() - bookmarkCache.at < 60_000) {
		return bookmarkCache.list;
	}
	const list = await invoke<Bookmark[]>("launcher_bookmarks", { browsers }).catch(() => []);
	bookmarkCache = { at: Date.now(), list, browsers: id };
	return list;
}

export async function searchBookmarks(
	ctx: SearchContext,
	search: string,
	isCurrent: () => boolean
): Promise<Result[]> {
	const q = search.trim();
	const list = await bookmarks(ctx.config.bookmarks.browsers);
	if (!isCurrent()) return [];
	const out: Result[] = [];
	for (const b of list) {
		const score = q
			? bestMatch(q, [b.name, b.url.replace(/^https?:\/\/(www\.)?/, "")], [1, 0.7])
			: 300;
		if (q && score < ctx.min) continue;
		const key = `bookmark:${b.url}`;
		out.push({
			key,
			title: b.name,
			subtitle: `${b.browser}${b.folder ? ` · ${b.folder}` : ""} · ${b.url}`,
			icon: { kind: "glyph", glyph: "bookmark" },
			score: 250 + score * 0.5 + ctx.boost(key),
			plugin: "bookmarks",
			run: () => open(b.url),
			copyText: b.url,
			actions: () => [
				{ key: "open", title: "Open in browser", glyph: "external", run: () => open(b.url) },
				{ key: "copy", title: "Copy URL", glyph: "copy", run: () => copy(b.url) }
			],
			replay: { type: "url", url: b.url, title: b.name }
		});
	}
	return out;
}
