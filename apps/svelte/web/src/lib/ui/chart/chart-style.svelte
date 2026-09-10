<script lang="ts">
	import { THEMES, type ChartConfig } from "./chart-utils.js";

	let { id, config }: { id: string; config: ChartConfig } = $props();

	const colorConfig = $derived(getColorConfig());

	const themeContents = $derived(getThemeContents());

	function getColorConfig() {
		return config ? Object.entries(config).filter(([, itemConfig]) => itemConfig.theme || itemConfig.color) : null;
	}

	function getThemeContents() {
		if (!colorConfig || !colorConfig.length) return;

		let themeContent = "";
		for (const [_theme, prefix] of Object.entries(THEMES)) {
			let content = `${prefix} [data-chart=${id}] {\n`;
			for (const [key, itemConfig] of colorConfig) {
				const theme = _theme as keyof typeof itemConfig.theme;
				const color = itemConfig.theme?.[theme] || itemConfig.color;
				if (color) {
					content += `\t--color-${key}: ${color};\n`;
				}
			}
			content += "}";

			themeContent += `${themeContent ? "\n" : ""}${content}`;
		}

		return themeContent;
	}
</script>

{#if themeContents}
	{#key id}
		<svelte:element this={"style"}>
			{themeContents}
		</svelte:element>
	{/key}
{/if}
