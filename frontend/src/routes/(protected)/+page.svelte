<script lang="ts">
    import type { PageData } from "./$types";
    import TmdbNowPlaying from "#lib/components/tmdb-now-playing.svelte";
    import ListCarousel from "#lib/components/list-carousel.svelte";
    import AnimatedToggle from "#lib/components/animated-toggle.svelte";
    import { Button } from "#lib/components/ui/button/index.js";
    import { MediaListStore, type BaseListItem } from "#lib/services/lists-cache.svelte.js";
    import PageShell from "#lib/components/page-shell.svelte";
    import HomeLanguageFilter from "#lib/components/home-language-filter.svelte";
    import { HomeLanguages } from "#lib/services/home-languages.svelte.js";
    import { fly } from "svelte/transition";
    import { cubicOut } from "svelte/easing";
    import { subscribeToRivenMediaEvents } from "#lib/services/riven-live-updates.js";
    import { gqlClient } from "#lib/graphql-client.js";
    import {
        getRecentItemsVariables,
        mapRecentItemsPage,
        RECENT_ITEMS_QUERY,
        type RecentListItem,
        type RecentItemsResponse,
        type RecentItemsSort
    } from "#lib/services/recent-items.js";

    let { data }: { data: PageData } = $props();

    const viewAllButtonClass =
        "text-muted-foreground border-white/10 bg-black/20 hover:bg-black/40 hover:text-foreground h-9 w-24 rounded-xl border text-xs font-bold backdrop-blur-md shadow-inner transition-all";

    const recentItemsStore = (key: string, sort: RecentItemsSort, initialData: RecentListItem[]) =>
        new MediaListStore<RecentListItem>({
            key,
            noCache: true,
            initialData,
            loader: async (page) => {
                const recentData = await gqlClient<RecentItemsResponse>(
                    RECENT_ITEMS_QUERY,
                    getRecentItemsVariables(page, sort)
                );
                return mapRecentItemsPage(recentData).items;
            }
        });
    // svelte-ignore state_referenced_locally
    const recentlyAddedStore = recentItemsStore("recentlyAdded", "date_desc", data.recentlyAdded);
    // svelte-ignore state_referenced_locally
    const recentlyDownloadedStore = recentItemsStore(
        "recentlyDownloaded",
        "downloaded_desc",
        data.recentlyDownloaded
    );

    // svelte-ignore state_referenced_locally
    const languageFilter = new HomeLanguages(data.user?.id);
    const TRENDING_QUERY = `query TrendingTmdb($type: String!, $timeWindow: String!, $page: Int) {
        trendingTmdb(type: $type, timeWindow: $timeWindow, page: $page) {
            results { id title posterPath mediaType year popularity originalLanguage }
        }
    }`;
    const ANILIST_TRENDING_QUERY = `query TrendingAnilist($page: Int!, $perPage: Int) {
        trendingAnilist(page: $page, perPage: $perPage) {
            results { id title posterPath mediaType year }
        }
    }`;
    type TrendingResult = {
        id: number;
        title?: string;
        name?: string;
        posterPath?: string;
        mediaType?: string;
        year?: number;
        popularity?: number;
        originalLanguage?: string | null;
    };
    type TrendingResponse = { trendingTmdb: { results: TrendingResult[] } };
    const mapTrending = (d: TrendingResponse) => d.trendingTmdb.results;

    const trendingMoviesStore = new MediaListStore<BaseListItem>({
        key: "trendingMovies",
        initialTimeWindow: "day",
        loader: (page, timeWindow) =>
            gqlClient<TrendingResponse>(TRENDING_QUERY, {
                type: "movie",
                timeWindow: timeWindow ?? "day",
                page
            }).then(mapTrending)
    });
    const trendingShowsStore = new MediaListStore<BaseListItem>({
        key: "trendingShows",
        initialTimeWindow: "day",
        loader: (page, timeWindow) =>
            gqlClient<TrendingResponse>(TRENDING_QUERY, {
                type: "tv",
                timeWindow: timeWindow ?? "day",
                page
            }).then(mapTrending)
    });
    const anilistTrendingStore = new MediaListStore<BaseListItem>({
        key: "anilistTrending",
        loader: (page) =>
            gqlClient<{
                trendingAnilist: {
                    results: Array<{
                        id: number;
                        title: string;
                        posterPath: string | null;
                        mediaType: string;
                        year: string;
                    }>;
                };
            }>(ANILIST_TRENDING_QUERY, { page, perPage: 20 }).then(
                (data) => data.trendingAnilist.results
            )
    });

    $effect(() => {
        return subscribeToRivenMediaEvents(() => {
            recentlyAddedStore.refresh();
            recentlyDownloadedStore.refresh();
        });
    });

    // The anime row is left out: AniList titles are Japanese by definition, so
    // a language filter would only ever empty it.
    const nowPlaying = $derived(data.nowPlaying.filter((item) => languageFilter.allows(item)));
    const trendingMovies = $derived(
        trendingMoviesStore.items.filter((item) => languageFilter.allows(item))
    );
    const trendingShows = $derived(
        trendingShowsStore.items.filter((item) => languageFilter.allows(item))
    );

    // A filtered row can be left with a handful of titles out of a 20-item
    // page, or none — and an empty row renders as loading skeletons. Fetch
    // further pages until it fills, within a bound, since a rare language may
    // never reach the target.
    const FILTERED_ROW_TARGET = 12;
    const FILTERED_ROW_MAX_ITEMS = 100;
    function topUp(store: MediaListStore<BaseListItem>, visible: number) {
        if (
            languageFilter.active &&
            store.initialized &&
            !store.loading &&
            store.hasMore &&
            visible < FILTERED_ROW_TARGET &&
            store.items.length < FILTERED_ROW_MAX_ITEMS
        ) {
            store.loadMore();
        }
    }
    $effect(() => topUp(trendingMoviesStore, trendingMovies.length));
    $effect(() => topUp(trendingShowsStore, trendingShows.length));
</script>

{#snippet listHeading(title: string)}
    <div class="flex items-center gap-3">
        <div class="bg-primary h-6 w-1 rounded-full shadow-[0_0_10px_rgba(var(--primary),0.5)]">
        </div>
        <h2 class="text-foreground text-2xl font-bold tracking-tight drop-shadow-md">
            {title}
        </h2>
    </div>
{/snippet}

<svelte:head>
    <title>Home - Riven</title>
</svelte:head>

<PageShell
    class="bg-background relative mt-0 flex min-h-screen flex-col overflow-x-hidden p-0 md:mt-0 md:p-0">
    <!-- Immersive Background -->
    <div class="pointer-events-none fixed inset-0 z-0">
        <div class="absolute inset-0 bg-linear-to-b from-zinc-900 via-zinc-950 to-black"></div>
        <div
            class="bg-primary/5 absolute top-[-20%] left-[-10%] h-150 w-150 rounded-full blur-[120px]">
        </div>
        <div
            class="absolute right-[-5%] bottom-[-10%] h-125 w-125 rounded-full bg-blue-500/5 blur-[100px]">
        </div>
    </div>

    <div class="relative z-10 flex w-full flex-col gap-10 pb-24 md:gap-12">
        <div
            in:fly|global={{ y: 20, duration: 400, delay: 0, easing: cubicOut }}
            class="w-full px-4 md:px-8">
            <!-- Remounted on a filter change: the carousel keeps its own slide
                 index, which a shorter list would leave pointing past the end. -->
            {#key languageFilter.languages.join()}
                <TmdbNowPlaying
                    data={nowPlaying}
                    heightClass="h-[50vh] min-h-[500px] max-h-[800px]" />
            {/key}
        </div>

        <div class="mx-auto flex w-full max-w-600 flex-col gap-12 px-6 md:px-12 lg:px-16">
            <div class="-mb-8 flex justify-end">
                <HomeLanguageFilter filter={languageFilter} />
            </div>

            {#if recentlyAddedStore.items.length}
                <div
                    class="flex flex-col gap-4"
                    in:fly|global={{ y: 20, duration: 400, delay: 100, easing: cubicOut }}>
                    {@render listHeading("Recently Added")}
                    <ListCarousel data={recentlyAddedStore.items} />
                </div>
            {/if}

            {#if recentlyDownloadedStore.items.length}
                <div
                    class="flex flex-col gap-4"
                    in:fly|global={{ y: 20, duration: 400, delay: 125, easing: cubicOut }}>
                    {@render listHeading("Recently Downloaded")}
                    <ListCarousel data={recentlyDownloadedStore.items} />
                </div>
            {/if}

            <div
                class="flex flex-col gap-4"
                in:fly|global={{ y: 20, duration: 400, delay: 150, easing: cubicOut }}>
                <div class="mb-1 flex items-center justify-between">
                    {@render listHeading("Trending Movies")}
                    <div class="flex items-center gap-3">
                        <AnimatedToggle
                            options={[
                                { label: "Today", value: "day" },
                                { label: "This Week", value: "week" }
                            ]}
                            value={trendingMoviesStore.timeWindow ?? "day"}
                            onchange={(v) =>
                                trendingMoviesStore.changeTimeWindow(v as "day" | "week")} />
                        <Button
                            class={viewAllButtonClass}
                            variant="ghost"
                            href="/lists/trending/movie">View All</Button>
                    </div>
                </div>
                <ListCarousel data={trendingMovies} />
            </div>

            <div
                class="flex flex-col gap-4"
                in:fly|global={{ y: 20, duration: 400, delay: 200, easing: cubicOut }}>
                <div class="mb-1 flex items-center justify-between">
                    {@render listHeading("Trending TV Shows")}
                    <div class="flex items-center gap-3">
                        <AnimatedToggle
                            options={[
                                { label: "Today", value: "day" },
                                { label: "This Week", value: "week" }
                            ]}
                            value={trendingShowsStore.timeWindow ?? "day"}
                            onchange={(v) =>
                                trendingShowsStore.changeTimeWindow(v as "day" | "week")} />
                        <Button class={viewAllButtonClass} variant="ghost" href="/lists/trending/tv"
                            >View All</Button>
                    </div>
                </div>
                <ListCarousel data={trendingShows} />
            </div>

            <div
                class="flex flex-col gap-4"
                in:fly|global={{ y: 20, duration: 400, delay: 250, easing: cubicOut }}>
                <div class="mb-1 flex items-center justify-between">
                    {@render listHeading("Trending Anime")}
                    <Button class={viewAllButtonClass} variant="ghost" href="/lists/trending/anime"
                        >View All</Button>
                </div>
                <ListCarousel data={anilistTrendingStore.items} indexer="anilist" />
            </div>
        </div>
    </div>
</PageShell>
