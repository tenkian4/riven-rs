<script lang="ts">
    import * as Chart from "#lib/components/ui/chart/index.js";
    import { PieChart } from "layerchart";
    import type { ProviderBreakdown } from "#lib/gql/schema.js";

    let { data }: { data: ProviderBreakdown[] } = $props();

    // `provider` is what served the download: "usenet", or the debrid store's
    // own name. "stremthru" is what a row written without a store name carries.
    const LABELS: Record<string, string> = {
        usenet: "Usenet",
        realdebrid: "Real-Debrid",
        alldebrid: "AllDebrid",
        torbox: "TorBox",
        premiumize: "Premiumize",
        debridlink: "Debrid-Link",
        stremthru: "Debrid"
    };
    const COLORS: Record<string, string> = {
        usenet: "#a78bfa",
        realdebrid: "#22d3ee",
        alldebrid: "#f472b6"
    };
    const FALLBACK_COLORS = ["#facc15", "#94a3b8", "#fb923c", "#4ade80"];

    const rows = $derived.by(() =>
        data
            .filter((item) => item.count > 0)
            .map((item, index) => ({
                label: LABELS[item.provider] ?? item.provider,
                value: item.count,
                color: COLORS[item.provider] ?? FALLBACK_COLORS[index % FALLBACK_COLORS.length]
            }))
    );
    const total = $derived(rows.reduce((sum, item) => sum + item.value, 0));
</script>

<section class="border-border/60 border-b py-8">
    <h2 class="text-base font-semibold">Download Sources</h2>

    {#if rows.length === 0}
        <p class="text-muted-foreground mt-6 text-sm">No completed downloads yet.</p>
    {:else}
        <div class="mt-6 grid items-center gap-6 sm:grid-cols-[14rem_minmax(0,1fr)]">
            <Chart.Container config={{}} class="mx-auto h-56 w-56">
                <PieChart
                    data={rows}
                    key="label"
                    value="value"
                    c="color"
                    innerRadius={-50}
                    cornerRadius={5}
                    padAngle={0.02}
                    padding={{ top: 16, bottom: 32, left: 32, right: 16 }}>
                    {#snippet tooltip()}
                        <Chart.Tooltip />
                    {/snippet}
                </PieChart>
            </Chart.Container>
            <div class="space-y-3">
                {#each rows as item (item.label)}
                    <div class="flex items-center gap-2">
                        <span
                            class="inline-block h-3 w-3 shrink-0 rounded-sm"
                            style="background-color: {item.color}"></span>
                        <span class="text-sm text-neutral-300">{item.label}</span>
                        <span class="ml-auto font-mono text-sm text-neutral-50">
                            {item.value.toLocaleString()}
                            <span class="text-muted-foreground">
                                ({Math.round((item.value / total) * 100)}%)
                            </span>
                        </span>
                    </div>
                {/each}
            </div>
        </div>
    {/if}
</section>
