<script lang="ts">
    import * as Popover from "#lib/components/ui/popover/index.js";
    import { Button } from "#lib/components/ui/button/index.js";
    import { Toggle } from "#lib/components/ui/toggle/index.js";
    import { Separator } from "#lib/components/ui/separator/index.js";
    import { LANGUAGE_OPTIONS } from "#lib/services/filter-store.svelte.js";
    import type { HomeLanguages } from "#lib/services/home-languages.svelte.js";
    import Languages from "@lucide/svelte/icons/languages";

    let { filter }: { filter: HomeLanguages } = $props();

    // "All Languages" is the absence of a filter here, so it is the Clear button.
    const options = LANGUAGE_OPTIONS.filter((option) => option.value !== "");

    const label = $derived(
        filter.active
            ? filter.languages.map((code) => code.toUpperCase()).join(", ")
            : "All languages"
    );
</script>

<Popover.Root>
    <Popover.Trigger>
        {#snippet child({ props })}
            <Button
                variant="ghost"
                class="text-muted-foreground hover:text-foreground h-9 gap-2 rounded-xl border border-white/10 bg-black/20 px-3 text-xs font-bold backdrop-blur-md hover:bg-black/40"
                {...props}>
                <Languages class="size-4" />
                <span class="max-w-40 truncate">{label}</span>
            </Button>
        {/snippet}
    </Popover.Trigger>
    <Popover.Content
        class="bg-popover w-80 rounded-2xl border-none shadow-2xl shadow-black/50"
        align="end">
        <div class="flex flex-col gap-3">
            <div class="flex items-center justify-between">
                <h4 class="font-semibold">Languages</h4>
                {#if filter.active}
                    <Button
                        variant="ghost"
                        size="sm"
                        onclick={() => filter.clear()}
                        class="h-7 px-2 text-xs">
                        Clear
                    </Button>
                {/if}
            </div>
            <p class="text-muted-foreground text-xs">
                Only show titles originally in these languages in the home page's trending rows.
                Saved in this browser.
            </p>
            <Separator />
            <div class="flex flex-wrap gap-1.5">
                {#each options as option (option.value)}
                    <Toggle
                        size="sm"
                        variant="outline"
                        class="h-7 px-2 text-xs"
                        pressed={filter.languages.includes(option.value)}
                        onPressedChange={() => filter.toggle(option.value)}>
                        {option.label}
                    </Toggle>
                {/each}
            </div>
        </div>
    </Popover.Content>
</Popover.Root>
