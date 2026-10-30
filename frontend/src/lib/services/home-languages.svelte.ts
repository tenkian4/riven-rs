import { browser } from "$app/env";

const STORAGE_PREFIX = "riven:home-languages";

/**
 * The original languages the home page's discovery rows are narrowed to.
 *
 * Kept in this browser's localStorage rather than on the backend, which has no
 * per-user preference storage. The key carries the user id so that two people
 * signing in on one browser each keep their own choice. Empty means no filter.
 */
export class HomeLanguages {
	readonly #storageKey: string;
	#languages = $state<string[]>([]);

	constructor(userId: string | null | undefined) {
		this.#storageKey = `${STORAGE_PREFIX}:${userId ?? "anonymous"}`;
		if (!browser) return;
		try {
			const stored = JSON.parse(localStorage.getItem(this.#storageKey) ?? "[]");
			if (Array.isArray(stored)) {
				this.#languages = stored.filter(
					(code): code is string => typeof code === "string",
				);
			}
		} catch {
			// malformed or unavailable storage; start unfiltered
		}
	}

	get languages(): string[] {
		return this.#languages;
	}

	get active(): boolean {
		return this.#languages.length > 0;
	}

	toggle(code: string): void {
		this.#set(
			this.#languages.includes(code)
				? this.#languages.filter((c) => c !== code)
				: [...this.#languages, code],
		);
	}

	clear(): void {
		this.#set([]);
	}

	/**
	 * Whether an item passes the filter. An item with no known language is
	 * kept: hiding it would make a gap in TMDB's data look like a preference.
	 */
	allows(item: object): boolean {
		const language = (item as { originalLanguage?: unknown }).originalLanguage;
		if (!this.active || typeof language !== "string") return true;
		return this.#languages.includes(language.toLowerCase());
	}

	#set(languages: string[]): void {
		this.#languages = languages;
		try {
			localStorage.setItem(this.#storageKey, JSON.stringify(languages));
		} catch {
			// storage unavailable; the choice lasts for this page only
		}
	}
}
