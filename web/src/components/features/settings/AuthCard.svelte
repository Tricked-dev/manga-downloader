<script lang="ts">
  import { ChevronDown } from "@lucide/svelte";
  import { Input } from "$lib/ui/input";
  import { Popover, PopoverContent, PopoverTrigger } from "$lib/ui/popover";
  import { Switch } from "$lib/ui/switch";
  import SettingField from "./SettingField.svelte";
  import SettingsCard from "./SettingsCard.svelte";
  import SettingsNotice from "./SettingsNotice.svelte";
  import { t } from "$lib/i18n";

  import type { SettingsMap } from "$lib/features/settings/types";

  let { settings = $bindable() } = $props<{ settings: SettingsMap }>();

  const oidcScopeOptions = [
    { value: "openid", descriptionKey: "app.settings.auth.openid" },
    { value: "profile", descriptionKey: "app.settings.auth.profile" },
    { value: "email", descriptionKey: "app.settings.auth.email" },
    { value: "offline_access", descriptionKey: "app.settings.auth.offlineAccess" },
    { value: "address", descriptionKey: "app.settings.auth.address" },
    { value: "phone", descriptionKey: "app.settings.auth.phone" },
    { value: "groups", descriptionKey: "app.settings.auth.groups" },
  ] as const;

  const authEnabled = $derived(settings.auth_enabled === "true");
  const oidcConfigured = $derived(
    Boolean(
      settings.auth_oidc_issuer_url?.trim() &&
      settings.auth_oidc_client_id?.trim(),
    ),
  );
  const selectedScopes = $derived(parseScopes(settings.auth_oidc_scopes));
  const visibleScopeOptions = $derived(getVisibleScopeOptions());
  const selectedScopesLabel = $derived(selectedScopes.length > 0 ? selectedScopes.join(" ") : $t("app.settings.auth.selectScopes"));

  function getVisibleScopeOptions() {
    const options: Array<{ value: string; descriptionKey: string }> = [...oidcScopeOptions];
    for (const scope of selectedScopes) {
      if (!oidcScopeOptions.some((option) => option.value === scope)) {
        options.push({ value: scope, descriptionKey: "app.settings.auth.customScope" });
      }
    }

    return options;
  }

  function parseScopes(value: string): string[] {
    const scopes: string[] = [];

    for (const scope of value.split(/\s+/).map((item) => item.trim()).filter(Boolean)) {
      if (!scopes.includes(scope)) {
        scopes.push(scope);
      }
    }

    return scopes;
  }

  function setScope(scope: string, selected: boolean) {
    const nextScopes = selected
      ? selectedScopes.includes(scope)
        ? selectedScopes
        : [...selectedScopes, scope]
      : selectedScopes.filter((value) => value !== scope);
    const orderedKnownScopes: string[] = oidcScopeOptions.map((option) => option.value);

    settings.auth_oidc_scopes = orderedKnownScopes
      .filter((value) => nextScopes.includes(value))
      .concat(nextScopes.filter((value) => !oidcScopeOptions.some((option) => option.value === value)))
      .join(" ");
  }

  function setAuthEnabled(checked: boolean) {
    settings.auth_enabled = checked ? "true" : "false";
  }

  function handleScopeChange(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    setScope(input.value, input.checked);
  }
</script>

<SettingsCard title={$t("app.settings.auth.title")} description={$t("app.settings.authDescription")}>
  <div class="flex items-start justify-between gap-4">
    <SettingField
      id="auth-enabled"
      label={$t("app.settings.auth.requireLogin")}
      hint={$t("app.settings.auth.requireLoginHint")}
      description={$t("app.settings.auth.requireLoginDescription")}
      class="space-y-0.5"
    />
    <Switch
      id="auth-enabled"
      checked={authEnabled}
      onCheckedChange={setAuthEnabled}
    />
  </div>

  {#if authEnabled && !oidcConfigured}
    <SettingsNotice>
      {$t("app.settings.auth.setupNotice")}
    </SettingsNotice>
  {/if}

  <SettingField
    id="auth-issuer-url"
    label={$t("app.settings.auth.issuerUrl")}
    hint={$t("app.settings.auth.issuerHint")}
  >
    <Input
      id="auth-issuer-url"
      type="url"
      class="h-9"
      bind:value={settings.auth_oidc_issuer_url}
      placeholder="https://id.example.com/realms/main"
    />
  </SettingField>

  <div class="grid gap-3 md:grid-cols-2">
    <SettingField id="auth-client-id" label={$t("app.settings.auth.clientId")}>
      <Input
        id="auth-client-id"
        type="text"
        class="h-9"
        bind:value={settings.auth_oidc_client_id}
        placeholder="manga-server"
      />
    </SettingField>

    <SettingField
      id="auth-client-secret"
      label={$t("app.settings.auth.clientSecret")}
      hint={$t("app.settings.auth.clientSecretHint")}
    >
      <Input
        id="auth-client-secret"
        type="password"
        class="h-9"
        bind:value={settings.auth_oidc_client_secret}
        autocomplete="new-password"
        placeholder={$t("app.settings.auth.clientSecretPlaceholder")}
      />
    </SettingField>
  </div>

  <div>
    <SettingField
      id="auth-scopes"
      label={$t("app.settings.auth.scopes")}
      hint={$t("app.settings.auth.scopesHint")}
    >
      <Popover>
        <PopoverTrigger
          id="auth-scopes"
          class="border-input bg-background dark:bg-input/30 dark:hover:bg-input/50 focus-visible:border-ring focus-visible:ring-ring/50 flex h-9 w-full items-center justify-between gap-2 rounded-lg border px-3 text-left text-sm transition-colors outline-none focus-visible:ring-3"
          aria-label={$t("app.settings.auth.selectScopes")}
        >
          <span class="min-w-0 flex-1 truncate">{selectedScopesLabel}</span>
          <ChevronDown class="size-4 shrink-0 text-muted-foreground" />
        </PopoverTrigger>
        <PopoverContent align="start" class="w-[min(26rem,calc(100vw-2rem))] gap-1 p-1.5">
          {#each visibleScopeOptions as scope (scope.value)}
            <label
              class="flex cursor-pointer items-start gap-2 rounded-md px-2 py-2 text-sm hover:bg-accent hover:text-accent-foreground"
            >
              <input
                type="checkbox"
                class="mt-0.5 size-4 accent-primary"
                value={scope.value}
                checked={selectedScopes.includes(scope.value)}
                onchange={handleScopeChange}
              />
              <span class="min-w-0">
                <span class="block font-medium">{scope.value}</span>
                <span class="block text-xs text-muted-foreground">{$t(scope.descriptionKey)}</span>
              </span>
            </label>
          {/each}
        </PopoverContent>
      </Popover>
    </SettingField>

  </div>
</SettingsCard>
