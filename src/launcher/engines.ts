/**
 * Which site each kind of search uses (Settings > Search Bar Pill > Search
 * Engines). Every kind has its own keyword; the web search ("*") also shows
 * under every search. Private engines come first.
 */

export type EngineCategory = "web" | "images" | "videos" | "maps" | "news" | "translate";

export interface EngineProvider {
	id: string;
	name: string;
	/** {q} is replaced with the search text. */
	url: string;
	/** Doesn't track searches or build a profile. */
	private?: boolean;
}

export interface CategoryInfo {
	id: EngineCategory;
	label: string;
	desc: string;
	defaultKeyword: string;
	defaultProvider: string;
	providers: EngineProvider[];
}

export const ENGINE_CATEGORIES: CategoryInfo[] = [
	{
		id: "web",
		label: "Web",
		desc: "Shown under every search, or type its keyword",
		defaultKeyword: "*",
		defaultProvider: "duckduckgo",
		providers: [
			{ id: "duckduckgo", name: "DuckDuckGo", url: "https://duckduckgo.com/?q={q}", private: true },
			{
				id: "startpage",
				name: "Startpage",
				url: "https://www.startpage.com/sp/search?query={q}",
				private: true
			},
			{
				id: "brave",
				name: "Brave Search",
				url: "https://search.brave.com/search?q={q}",
				private: true
			},
			{ id: "qwant", name: "Qwant", url: "https://www.qwant.com/?q={q}", private: true },
			{ id: "ecosia", name: "Ecosia", url: "https://www.ecosia.org/search?q={q}" },
			{ id: "kagi", name: "Kagi", url: "https://kagi.com/search?q={q}" },
			{ id: "google", name: "Google", url: "https://www.google.com/search?q={q}" },
			{ id: "bing", name: "Bing", url: "https://www.bing.com/search?q={q}" }
		]
	},
	{
		id: "images",
		label: "Images",
		desc: "Picture search",
		defaultKeyword: "image",
		defaultProvider: "duckduckgo",
		providers: [
			{
				id: "duckduckgo",
				name: "DuckDuckGo Images",
				url: "https://duckduckgo.com/?q={q}&ia=images&iax=images",
				private: true
			},
			{
				id: "startpage",
				name: "Startpage Images",
				url: "https://www.startpage.com/sp/search?query={q}&cat=images",
				private: true
			},
			{
				id: "brave",
				name: "Brave Images",
				url: "https://search.brave.com/images?q={q}",
				private: true
			},
			{
				id: "qwant",
				name: "Qwant Images",
				url: "https://www.qwant.com/?q={q}&t=images",
				private: true
			},
			{ id: "google", name: "Google Images", url: "https://www.google.com/search?q={q}&tbm=isch" },
			{ id: "bing", name: "Bing Images", url: "https://www.bing.com/images/search?q={q}" }
		]
	},
	{
		id: "videos",
		label: "Videos",
		desc: "Video search",
		defaultKeyword: "video",
		defaultProvider: "duckduckgo",
		providers: [
			{
				id: "duckduckgo",
				name: "DuckDuckGo Videos",
				url: "https://duckduckgo.com/?q={q}&ia=videos&iax=videos",
				private: true
			},
			{
				id: "startpage",
				name: "Startpage Videos",
				url: "https://www.startpage.com/sp/search?query={q}&cat=video",
				private: true
			},
			{
				id: "brave",
				name: "Brave Videos",
				url: "https://search.brave.com/videos?q={q}",
				private: true
			},
			{ id: "youtube", name: "YouTube", url: "https://www.youtube.com/results?search_query={q}" },
			{ id: "google", name: "Google Videos", url: "https://www.google.com/search?q={q}&tbm=vid" },
			{ id: "bing", name: "Bing Videos", url: "https://www.bing.com/videos/search?q={q}" }
		]
	},
	{
		id: "maps",
		label: "Maps",
		desc: "Places and directions",
		defaultKeyword: "maps",
		defaultProvider: "duckduckgo",
		providers: [
			{
				id: "duckduckgo",
				name: "DuckDuckGo Maps",
				url: "https://duckduckgo.com/?q={q}&ia=maps&iaxm=maps",
				private: true
			},
			{
				id: "openstreetmap",
				name: "OpenStreetMap",
				url: "https://www.openstreetmap.org/search?query={q}",
				private: true
			},
			{ id: "apple", name: "Apple Maps", url: "https://maps.apple.com/?q={q}" },
			{ id: "google", name: "Google Maps", url: "https://www.google.com/maps/search/{q}" },
			{ id: "bing", name: "Bing Maps", url: "https://www.bing.com/maps?q={q}" }
		]
	},
	{
		id: "news",
		label: "News",
		desc: "News search",
		defaultKeyword: "news",
		defaultProvider: "duckduckgo",
		providers: [
			{
				id: "duckduckgo",
				name: "DuckDuckGo News",
				url: "https://duckduckgo.com/?q={q}&ia=news&iar=news",
				private: true
			},
			{
				id: "startpage",
				name: "Startpage News",
				url: "https://www.startpage.com/sp/search?query={q}&cat=news",
				private: true
			},
			{
				id: "brave",
				name: "Brave News",
				url: "https://search.brave.com/news?q={q}",
				private: true
			},
			{ id: "google", name: "Google News", url: "https://news.google.com/search?q={q}" },
			{ id: "bing", name: "Bing News", url: "https://www.bing.com/news/search?q={q}" }
		]
	},
	{
		id: "translate",
		label: "Translate",
		desc: "Translate text into English",
		defaultKeyword: "translate",
		defaultProvider: "deepl",
		providers: [
			{ id: "deepl", name: "DeepL", url: "https://www.deepl.com/translator#auto/en/{q}" },
			{
				id: "google",
				name: "Google Translate",
				url: "https://translate.google.com/?sl=auto&tl=en&text={q}"
			},
			{ id: "bing", name: "Bing Translator", url: "https://www.bing.com/translator?text={q}" }
		]
	}
];

export interface EngineSetting {
	provider: string;
	keyword: string;
	enabled: boolean;
}

export type EngineSettings = Record<EngineCategory, EngineSetting>;

export const DEFAULT_ENGINES = Object.fromEntries(
	ENGINE_CATEGORIES.map((c) => [
		c.id,
		{ provider: c.defaultProvider, keyword: c.defaultKeyword, enabled: true }
	])
) as EngineSettings;

export function providerFor(category: CategoryInfo, id: string): EngineProvider {
	return category.providers.find((p) => p.id === id) ?? category.providers[0];
}

/**
 * Search engine entries of the old single list that the per-kind engines
 * replace; they are dropped from "Other Sites" once, when settings move over.
 */
export const REPLACED_SOURCE_IDS = new Set([
	"google",
	"google-images",
	"google-maps",
	"google-translate",
	"duckduckgo",
	"bing",
	"yahoo",
	"i-m-feeling-lucky"
]);
