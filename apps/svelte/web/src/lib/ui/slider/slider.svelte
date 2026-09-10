<script lang="ts">
 	import { Slider as SliderPrimitive } from "bits-ui";
	import { cn } from '$lib/utils.js';
import type { WithoutChildrenOrChild } from '$lib/utils.js';

	let {
		ref = $bindable(null),
		value = $bindable(),
		orientation = "horizontal",
		class: className,
		...restProps
	}: WithoutChildrenOrChild<SliderPrimitive.RootProps> = $props();
</script>

<!--
Discriminated Unions + Destructing (required for bindable) do not
get along, so we shut typescript up by casting `value` to `never`.
-->
<SliderPrimitive.Root
	bind:ref
	bind:value={value as never}
	data-slot="slider"
	{orientation}
	class={cn(
		"data-vertical:min-h-40 relative flex w-full touch-none items-center select-none py-2 data-disabled:opacity-50 data-vertical:h-full data-vertical:w-auto data-vertical:flex-col",
		className
	)}
	{...restProps}
>
	{#snippet children({ thumbItems })}
		<span
			data-slot="slider-track"
			data-orientation={orientation}
			class={cn(
				"relative grow overflow-hidden rounded-full border border-border/70 bg-muted/90 shadow-[inset_0_0_0_1px_rgba(255,255,255,0.05)] data-horizontal:h-2 data-horizontal:w-full data-vertical:h-full data-vertical:w-2"
			)}
		>
			<SliderPrimitive.Range
				data-slot="slider-range"
				class={cn(
					"absolute select-none bg-primary shadow-[0_0_18px_color-mix(in_oklab,var(--color-primary)_45%,transparent)] data-horizontal:h-full data-vertical:w-full"
				)}
			/>
		</span>
		{#each thumbItems as thumb (thumb)}
			<SliderPrimitive.Thumb
				data-slot="slider-thumb"
				index={thumb.index}
				class="relative block size-4 shrink-0 select-none rounded-full border border-primary bg-card shadow-[0_0_0_2px_var(--color-background),0_2px_10px_rgba(0,0,0,0.45)] transition-[color,box-shadow,transform] after:absolute after:-inset-2 hover:scale-105 hover:ring-3 hover:ring-ring/40 focus-visible:scale-105 focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:outline-hidden active:scale-100 disabled:pointer-events-none disabled:opacity-50"
			/>
		{/each}
	{/snippet}
</SliderPrimitive.Root>
