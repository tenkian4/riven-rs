import type { PageLoad } from "./$types";
import { createScopedLogger } from "#lib/logger.js";
import { gqlClient } from "#lib/graphql-client.js";
import { fetchTmdbTrending } from "#lib/services/backend-metadata.js";
import {
	getRecentItemsVariables,
	mapRecentItemsPage,
	RECENT_ITEMS_QUERY,
	type RecentListItem,
	type RecentItemsResponse,
	type RecentItemsSort,
} from "#lib/services/recent-items.js";

const logger = createScopedLogger("home");

export const load: PageLoad = async () => {
	// Auth gating lives in the protected layout.

	try {
		const trendingResults = await fetchTmdbTrending({
			type: "all",
			timeWindow: "day",
			page: 1,
		});

		const fetchRecent = async (
			sort: RecentItemsSort,
		): Promise<RecentListItem[]> => {
			try {
				const recentData = await gqlClient<RecentItemsResponse>(
					RECENT_ITEMS_QUERY,
					getRecentItemsVariables(1, sort),
				);
				return mapRecentItemsPage(recentData).items;
			} catch (err) {
				logger.error(`Error fetching recent items (${sort}):`, err);
				return [];
			}
		};
		const [recentlyAdded, recentlyDownloaded] = await Promise.all([
			fetchRecent("date_desc"),
			fetchRecent("downloaded_desc"),
		]);

		// Filter to only movies and TV shows with backdrops
		const tmdbResults = trendingResults;
		const filtered = tmdbResults.filter(
			(item) =>
				(item.mediaType === "movie" || item.mediaType === "tv") &&
				item.backdropPath,
		);

		return { nowPlaying: filtered, recentlyAdded, recentlyDownloaded };
	} catch (err) {
		logger.error("Error fetching now playing data:", err);
		return { nowPlaying: [], recentlyAdded: [], recentlyDownloaded: [] };
	}
};
