<script lang="ts">
  import { onMount } from "svelte";
  import { Check, Copy, Loader2, Plus, Trash2 } from "@lucide/svelte";
  import { Button } from "$lib/ui/button";
  import { Input } from "$lib/ui/input";
  import SettingsCard from "./SettingsCard.svelte";
  import SettingsNotice from "./SettingsNotice.svelte";
  import { t } from "$lib/i18n";

  interface ApiToken {
    id: string;
    name: string;
    created_at: string;
    last_used_at: string | null;
  }

  let tokens = $state<ApiToken[]>([]);
  let newName = $state("");
  let created = $state<{ name: string; token: string } | null>(null);
  let busy = $state(false);
  let copied = $state(false);
  let errorMessage = $state("");

  // Timestamps are stored as zero-padded epoch milliseconds.
  function formatStamp(value: string | null): string {
    if (!value) {
      return $t("app.settings.tokensNeverUsed");
    }
    const parsed = Number(value);
    return Number.isFinite(parsed) ? new Date(parsed).toLocaleString() : value;
  }

  async function load() {
    try {
      const response = await fetch("/v1/tokens");
      if (!response.ok) {
        throw new Error($t("app.settings.tokensLoadFailed"));
      }
      tokens = (await response.json()).items ?? [];
    } catch (caught) {
      errorMessage = caught instanceof Error ? caught.message : $t("app.settings.tokensLoadFailed");
    }
  }

  async function createToken() {
    if (!newName.trim()) {
      return;
    }
    busy = true;
    errorMessage = "";
    copied = false;
    try {
      const response = await fetch("/v1/tokens", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ name: newName.trim() }),
      });
      if (!response.ok) {
        throw new Error($t("app.settings.tokensCreateFailed"));
      }
      const body = await response.json();
      created = { name: body.name, token: body.token };
      newName = "";
      await load();
    } catch (caught) {
      errorMessage = caught instanceof Error ? caught.message : $t("app.settings.tokensCreateFailed");
    } finally {
      busy = false;
    }
  }

  async function deleteToken(id: string) {
    busy = true;
    errorMessage = "";
    try {
      const response = await fetch(`/v1/tokens/${encodeURIComponent(id)}`, { method: "DELETE" });
      if (!response.ok) {
        throw new Error($t("app.settings.tokensDeleteFailed"));
      }
      await load();
    } catch (caught) {
      errorMessage = caught instanceof Error ? caught.message : $t("app.settings.tokensDeleteFailed");
    } finally {
      busy = false;
    }
  }

  async function copyCreated() {
    if (!created) {
      return;
    }
    try {
      await navigator.clipboard.writeText(created.token);
      copied = true;
      window.setTimeout(() => {
        copied = false;
      }, 1800);
    } catch {
      errorMessage = $t("app.settings.apiKeyCopyFailed");
    }
  }

  onMount(load);
</script>

<SettingsCard title={$t("app.settings.tokens")} description={$t("app.settings.tokensDescription")}>
  <div class="flex flex-wrap items-center gap-2">
    <Input
      class="h-9 min-w-0 flex-1"
      bind:value={newName}
      placeholder={$t("app.settings.tokensNamePlaceholder")}
      disabled={busy}
    />
    <Button
      variant="outline"
      size="sm"
      type="button"
      disabled={busy || !newName.trim()}
      onclick={createToken}
    >
      {#if busy}
        <Loader2 class="size-3.5 animate-spin" />
      {:else}
        <Plus class="size-3.5" />
      {/if}
      {$t("app.settings.tokensCreate")}
    </Button>
  </div>

  {#if created}
    <SettingsNotice>
      <div class="flex flex-wrap items-center gap-2">
        <span>{$t("app.settings.tokensCreatedOnce")}</span>
        <code class="min-w-0 break-all font-mono text-xs">{created.token}</code>
        <Button variant="outline" size="sm" type="button" onclick={copyCreated}>
          {#if copied}
            <Check class="size-3.5" />
            {$t("app.settings.apiKeyCopied")}
          {:else}
            <Copy class="size-3.5" />
            {$t("app.settings.apiKeyCopy")}
          {/if}
        </Button>
      </div>
    </SettingsNotice>
  {/if}

  {#if tokens.length === 0}
    <p class="text-xs text-muted-foreground">{$t("app.settings.tokensEmpty")}</p>
  {:else}
    <ul class="divide-y">
      {#each tokens as token (token.id)}
        <li class="flex items-center justify-between gap-3 py-2">
          <div class="min-w-0">
            <p class="truncate text-sm font-medium">{token.name}</p>
            <p class="text-xs text-muted-foreground">
              {$t("app.settings.tokensCreatedAt")}: {formatStamp(token.created_at)} ·
              {$t("app.settings.tokensLastUsed")}: {formatStamp(token.last_used_at)}
            </p>
          </div>
          <Button
            variant="ghost"
            size="sm"
            type="button"
            disabled={busy}
            aria-label={$t("app.settings.tokensDelete")}
            onclick={() => deleteToken(token.id)}
          >
            <Trash2 class="size-3.5" />
          </Button>
        </li>
      {/each}
    </ul>
  {/if}

  {#if errorMessage}
    <SettingsNotice>{errorMessage}</SettingsNotice>
  {/if}
</SettingsCard>
