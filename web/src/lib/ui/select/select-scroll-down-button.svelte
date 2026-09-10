<script lang="ts">
	import { Select as SelectPrimitive } from "bits-ui";
	import { cn } from '$lib/utils.js';
import type { WithoutChildrenOrChild } from '$lib/utils.js';

	let {
		ref = $bindable(null),
		class: className,
		...restProps
	}: WithoutChildrenOrChild<SelectPrimitive.ScrollDownButtonProps> = $props();

	const scrollButtonClass = $derived(cn("bg-popover z-10 flex cursor-default items-center justify-center py-1 bottom-0 w-full", className));
</script>

<SelectPrimitive.ScrollDownButton
	bind:ref
	data-slot="select-scroll-down-button"
	class={scrollButtonClass}
	{...restProps}
>
	<svg class="size-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
		<path d="m6 9 6 6 6-6" />
	</svg>
</SelectPrimitive.ScrollDownButton>
