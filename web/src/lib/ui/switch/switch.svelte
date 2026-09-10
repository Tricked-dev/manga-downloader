<script lang="ts">
 	import { Switch as SwitchPrimitive } from "bits-ui";
	import { cn } from '$lib/utils.js';
import type { WithoutChildrenOrChild } from '$lib/utils.js';

	let {
		ref = $bindable(null),
		class: className,
		checked = $bindable(false),
		size = "default",
		...restProps
	}: WithoutChildrenOrChild<SwitchPrimitive.RootProps> & {
		size?: "sm" | "default";
	} = $props();
</script>

<SwitchPrimitive.Root
	bind:ref
	bind:checked
	data-slot="switch"
	data-size={size}
	class={cn(
		"peer group/switch relative inline-flex shrink-0 items-center rounded-full border border-[color:color-mix(in_oklab,var(--color-foreground)_18%,var(--color-border))] bg-muted shadow-[inset_0_0_0_1px_rgba(255,255,255,0.08)] outline-none transition-[background-color,border-color,box-shadow] duration-150 ease-linear after:absolute after:-inset-x-3 after:-inset-y-2 data-[size=default]:h-[18.4px] data-[size=default]:w-[32px] data-[size=sm]:h-[14px] data-[size=sm]:w-[24px] data-[state=checked]:border-primary data-[state=checked]:bg-primary data-[state=checked]:shadow-[0_0_18px_color-mix(in_oklab,var(--color-primary)_38%,transparent)] data-disabled:cursor-not-allowed data-disabled:opacity-50 focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 aria-invalid:border-destructive aria-invalid:ring-3 aria-invalid:ring-destructive/20 dark:aria-invalid:border-destructive/50 dark:aria-invalid:ring-destructive/40",
		className
	)}
	{...restProps}
>
	<SwitchPrimitive.Thumb
		data-slot="switch-thumb"
		class="pointer-events-none block size-4 translate-x-0 rounded-full bg-foreground shadow-[0_1px_3px_rgba(0,0,0,0.35)] ring-1 ring-background/10 transition-[transform,background-color,box-shadow] duration-150 ease-linear group-data-[size=sm]/switch:size-3 group-data-[state=checked]/switch:translate-x-[calc(100%-2px)] group-data-[state=checked]/switch:bg-primary-foreground rtl:group-data-[state=checked]/switch:translate-x-[calc(-100%+2px)]"
	/>
</SwitchPrimitive.Root>
