<script lang="ts">
  import { invalidateAll } from "$app/navigation";
  import { Copy, Save } from "@lucide/svelte";
  import CheckboxRow from "@manga-server/ui/components/checkbox-row";
  import FormField from "@manga-server/ui/components/form-field";
  import SectionHeader from "@manga-server/ui/components/section-header";
  import type { AddNotification, BetterAuthSettings, KanidmEntry } from "$lib/kanidm/types";
  import { appDisplayName, attr, callbackUrl, entryName, firstAttr, isPublicClient, readableError } from "$lib/kanidm/utils";
  import EntrySelect from "./EntrySelect.svelte";

  let {
    apps,
    configured,
    origin,
    settings,
    addNotification,
  }: {
    addNotification: AddNotification;
    apps: KanidmEntry[];
    configured: boolean;
    origin: string;
    settings: BetterAuthSettings;
  } = $props();

  let selectedAppName = $derived(settings.auth_oidc_client_id || (apps[0] ? entryName(apps[0]) : ""));
  let providerId = $derived(settings.auth_oidc_provider_id || "oidc");
  let scopes = $derived(settings.auth_oidc_scopes || "openid profile email");
  let issuerUrl = $derived(settings.auth_oidc_issuer_url || "");
  let clientSecret = $derived(settings.auth_oidc_client_secret || "");
  let enableAuth = $derived((settings.auth_enabled ?? "false") === "true");
  let saving = $state(false);

  const selectedApp = $derived(apps.find((app) => entryName(app) === selectedAppName));
  const redirectUrl = $derived(callbackUrl(origin, providerId));

  async function copyRedirect() {
    await navigator.clipboard.writeText(redirectUrl);
    addNotification("success", "Copied the Better Auth callback URL.");
  }

  async function saveSettings() {
    if (!configured) {
      addNotification("error", "Configure MANGA_SERVER_BASE_URL before syncing Better Auth settings.");
      return;
    }

    saving = true;
    try {
      const response = await fetch("/api/manga-settings", {
        body: JSON.stringify({
          auth_enabled: enableAuth ? "true" : "false",
          auth_oidc_client_id: selectedAppName,
          auth_oidc_client_secret: isPublicClient(selectedApp) ? "" : clientSecret,
          auth_oidc_issuer_url: issuerUrl,
          auth_oidc_provider_id: providerId,
          auth_oidc_scopes: scopes,
        }),
        headers: { "content-type": "application/json" },
        method: "PUT",
      });
      const body = await response.json().catch(() => null);
      if (!response.ok) {
        addNotification("error", readableError(body, "Could not save Better Auth settings."));
        return;
      }
      addNotification("success", "Saved Better Auth OIDC settings to Manga Server.");
      await invalidateAll();
    } finally {
      saving = false;
    }
  }
</script>

<section class="space-y-4">
  <div class="panel p-4">
    <SectionHeader
      title="Better Auth sync"
      description="Save a Kanidm OAuth2 application into the Manga Server settings that the existing frontend reads for Better Auth's generic OIDC provider."
    >
      {#snippet action()}
        <button class="button button-primary" type="button" disabled={saving || !configured} onclick={saveSettings}>
          <Save class="size-4" />
          {saving ? "Saving" : "Save settings"}
        </button>
      {/snippet}
    </SectionHeader>

    <div class="mt-5 grid gap-4 lg:grid-cols-2">
      <FormField label="Kanidm OAuth2 application">
        <EntrySelect entries={apps} placeholder="" showName bind:value={selectedAppName} />
      </FormField>

      <FormField label="Issuer URL">
        <input class="control font-mono" type="url" placeholder="https://id.example.com/oauth2/openid" bind:value={issuerUrl} />
      </FormField>

      <FormField label="Provider ID">
        <input class="control font-mono" bind:value={providerId} />
      </FormField>

      <FormField label="Scopes">
        <input class="control font-mono" bind:value={scopes} />
      </FormField>

      <FormField label="Client secret">
        <input
          class="control font-mono"
          disabled={isPublicClient(selectedApp)}
          placeholder={isPublicClient(selectedApp) ? "Public clients do not use a secret" : "Paste copied Kanidm client secret"}
          type="password"
          bind:value={clientSecret}
        />
      </FormField>

      <div class="self-end">
        <CheckboxRow bind:checked={enableAuth}>
          Require login in the Manga Server frontend after restart
        </CheckboxRow>
      </div>
    </div>
  </div>

  <div class="panel p-4">
    <div class="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
      <div>
        <p class="text-sm font-medium">Better Auth callback URL</p>
        <code class="mt-1 block break-all text-xs text-muted-foreground">{redirectUrl}</code>
      </div>
      <button class="button button-outline" type="button" onclick={copyRedirect}>
        <Copy class="size-4" />
        Copy
      </button>
    </div>
    {#if selectedApp}
      <div class="mt-4 grid gap-2 text-xs text-muted-foreground sm:grid-cols-2">
        <p>Kanidm origins: {attr(selectedApp, "oauth2_rs_origin").join(", ") || "none"}</p>
        <p>Landing URL: {firstAttr(selectedApp, "oauth2_rs_origin_landing", "none")}</p>
      </div>
    {/if}
  </div>
</section>
