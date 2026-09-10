<script lang="ts" module>
	import { tv } from 'tailwind-variants';
import type { VariantProps } from 'tailwind-variants';

	export const badgeVariants = tv({
		base: "h-5 gap-1 rounded-4xl border border-transparent px-2 py-0.5 text-xs font-medium transition-all has-data-[icon=inline-end]:pr-1.5 has-data-[icon=inline-start]:pl-1.5 [&>svg]:size-3! focus-visible:border-ring focus-visible:ring-ring/50 aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 aria-invalid:border-destructive group/badge inline-flex w-fit shrink-0 items-center justify-center overflow-hidden whitespace-nowrap transition-colors focus-visible:ring-[3px] [&>svg]:pointer-events-none",
		defaultVariants: {
			variant: "default",
		},
		variants: {
			variant: {
				danger: "border-none bg-destructive/15 text-destructive",
				default: "bg-primary text-primary-foreground [a]:hover:bg-primary/80",
				destructive: "bg-destructive/10 [a]:hover:bg-destructive/20 focus-visible:ring-destructive/20 dark:focus-visible:ring-destructive/40 text-destructive dark:bg-destructive/20",
				ghost: "hover:bg-muted hover:text-muted-foreground dark:hover:bg-muted/50",
				info: "border-none bg-sky-500/15 text-sky-500",
				link: "text-primary underline-offset-4 hover:underline",
				muted: "border-border bg-muted text-muted-foreground",
				outline: "border-border text-foreground [a]:hover:bg-muted [a]:hover:text-muted-foreground",
				secondary: "bg-secondary text-secondary-foreground [a]:hover:bg-secondary/80",
				success: "border-none bg-primary/15 text-primary",
				warning: "border-none bg-amber-500/15 text-amber-500",
			},
		},
	});

	export type BadgeVariant = VariantProps<typeof badgeVariants>["variant"];
</script>

<script lang="ts">
	import type { HTMLAnchorAttributes } from "svelte/elements";
	import { cn } from '$lib/utils.js';
import type { WithElementRef } from '$lib/utils.js';

	let {
		ref = $bindable(null),
		href,
		class: className,
		variant = "default",
		children,
		...restProps
	}: WithElementRef<HTMLAnchorAttributes> & {
		variant?: BadgeVariant;
	} = $props();
</script>

<svelte:element
	this={href ? "a" : "span"}
	bind:this={ref}
	data-slot="badge"
	{href}
	class={cn(badgeVariants({ variant }), className)}
	{...restProps}
>
	{@render children?.()}
</svelte:element>
