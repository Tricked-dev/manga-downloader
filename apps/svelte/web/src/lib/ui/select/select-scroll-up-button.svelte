<script lang="ts">
	import { Select as SelectPrimitive } from "bits-ui";
	import { cn } from '$lib/utils.js';
import type { WithoutChildrenOrChild } from '$lib/utils.js';

	let {
		ref = $bindable(null),
		class: className,
		...restProps
	}: WithoutChildrenOrChild<SelectPrimitive.ScrollUpButtonProps> = $props();

	const scrollButtonClass = $derived(cn("bg-popover z-10 flex cursor-default items-center justify-center py-1 top-0 w-full", className));
</script>

<SelectPrimitive.ScrollUpButton
	bind:ref
	data-slot="select-scroll-up-button"
	class={scrollButtonClass}
	{...restProps}
>
	<svg class="size-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
		<path d="m18 15-6-6-6 6" />
	</svg>
</SelectPrimitive.ScrollUpButton>
