<script lang="ts">
  import { invalidateAll } from "$app/navigation";
  import { ChevronDown, ExternalLink, Globe2, KeyRound, LockKeyhole, Plus, Save, Trash2, Upload } from "@lucide/svelte";
  import CheckboxRow from "@manga-server/ui/components/checkbox-row";
  import EmptyPanel from "@manga-server/ui/components/empty-panel";
  import FormField from "@manga-server/ui/components/form-field";
  import SectionHeader from "@manga-server/ui/components/section-header";
  import type { AddNotification, KanidmEntry } from "$lib/kanidm/types";
  import {
    appDisplayName,
    attr,
    entryName,
    firstAttr,
    isPublicClient,
    kanidmRequest,
    parseCommaList,
    parseMultilineUrls,
    readableError,
    runKanidmMutation,
  } from "$lib/kanidm/utils";
  import EntrySelect from "./EntrySelect.svelte";

  type EditDraft = {
    displayName: string;
    origin: string;
    redirectUrls: string;
    requirePkce: boolean;
    secureCrypto: boolean;
  };

  let {
    apps,
    groups,
    addNotification,
  }: {
    addNotification: AddNotification;
    apps: KanidmEntry[];
    groups: KanidmEntry[];
  } = $props();

  let showCreateForm = $state(false);
  let editing = $state<Record<string, EditDraft>>({});
  let createValues = $state({
    displayName: "",
    name: "",
    origin: "",
    redirectUrls: "",
    type: "basic" as "basic" | "public",
  });
  let scopeForm = $state({ appName: "", groupName: "", scopes: "openid, profile, email, groups" });
  let claimForm = $state({ appName: "", claimName: "groups", groupName: "", claims: "name", joinStrategy: "array" as "array" | "csv" | "ssv" });
  let uploading = $state("");

  function startEditing(app: KanidmEntry) {
    const name = entryName(app);
    editing[name] = {
      displayName: appDisplayName(app),
      origin: firstAttr(app, "oauth2_rs_origin_landing"),
      redirectUrls: attr(app, "oauth2_rs_origin").join("\n"),
      requirePkce: firstAttr(app, "oauth2_allow_insecure_client_disable_pkce") !== "true",
      secureCrypto: firstAttr(app, "oauth2_jwt_legacy_crypto_enable") !== "true",
    };
  }

  function cancelEditing(appName: string) {
    delete editing[appName];
  }

  async function createApplication() {
    const response = await runKanidmMutation(fetch, addNotification, {
        body: {
          attrs: {
            displayname: [createValues.displayName.trim()],
            name: [createValues.name.trim().toLowerCase()],
            oauth2_rs_origin: parseMultilineUrls(createValues.redirectUrls),
            oauth2_rs_origin_landing: [createValues.origin.trim()],
          },
        },
        method: "POST",
        path: `v1/oauth2/_${createValues.type}`,
      },
      {
        error: "Could not create OAuth2 application.",
        success: `Created ${createValues.name}.`,
      },
    );

    if (response) {
      createValues = { displayName: "", name: "", origin: "", redirectUrls: "", type: "basic" };
      showCreateForm = false;
      await invalidateAll();
    }
  }

  async function saveApplication(appName: string) {
    const draft = editing[appName];
    if (!draft) {
      return;
    }

    const response = await runKanidmMutation(fetch, addNotification, {
      body: {
        attrs: {
          displayname: [draft.displayName.trim()],
          oauth2_allow_insecure_client_disable_pkce: [draft.requirePkce ? "false" : "true"],
          oauth2_jwt_legacy_crypto_enable: [draft.secureCrypto ? "false" : "true"],
          oauth2_rs_origin: parseMultilineUrls(draft.redirectUrls),
          oauth2_rs_origin_landing: [draft.origin.trim()],
        },
      },
      method: "PATCH",
      path: `v1/oauth2/${appName}`,
    }, {
      error: "Could not update OAuth2 application.",
      success: `Updated ${appName}.`,
    });

    if (response) {
      delete editing[appName];
      await invalidateAll();
    }
  }

  async function deleteApplication(appName: string) {
    if (!confirm(`Delete OAuth2 application ${appName}?`)) {
      return;
    }
    const response = await runKanidmMutation(fetch, addNotification, { method: "DELETE", path: `v1/oauth2/${appName}` }, {
      error: "Could not delete OAuth2 application.",
      success: `Deleted ${appName}.`,
    });
    if (response) {
      await invalidateAll();
    }
  }

  async function copySecret(appName: string) {
    const response = await kanidmRequest<string>(fetch, {
      method: "GET",
      path: `v1/oauth2/${appName}/_basic_secret`,
    });
    if (response.status === 200 && response.body) {
      await navigator.clipboard.writeText(response.body);
      addNotification("success", `Copied secret for ${appName}.`);
    } else {
      addNotification("error", readableError(response.body, "Could not fetch client secret."));
    }
  }

  async function addScopeMap() {
    const response = await runKanidmMutation(fetch, addNotification, {
      body: parseCommaList(scopeForm.scopes),
      method: "POST",
      path: `v1/oauth2/${scopeForm.appName}/_scopemap/${scopeForm.groupName.trim()}`,
    }, {
      error: "Could not add scope map.",
      success: `Added scope map for ${scopeForm.groupName}.`,
    });
    if (response) {
      scopeForm = { ...scopeForm, groupName: "" };
      await invalidateAll();
    }
  }

  async function deleteScopeMap(appName: string, groupName: string) {
    const response = await runKanidmMutation(fetch, addNotification, {
      method: "DELETE",
      path: `v1/oauth2/${appName}/_scopemap/${groupName}`,
    }, {
      error: "Could not remove scope map.",
      success: `Removed scope map for ${groupName}.`,
    });
    if (response) {
      await invalidateAll();
    }
  }

  async function addClaimMap() {
    const response = await runKanidmMutation(fetch, addNotification, {
      body: {
        join_strategy: claimForm.joinStrategy,
        values: parseCommaList(claimForm.claims),
      },
      method: "POST",
      path: `v1/oauth2/${claimForm.appName}/_claimmap/${claimForm.claimName.trim()}/${claimForm.groupName.trim()}`,
    }, {
      error: "Could not add claim map.",
      success: `Added ${claimForm.claimName} claim map.`,
    });
    if (response) {
      claimForm = { ...claimForm, groupName: "", claims: "" };
      await invalidateAll();
    }
  }

  async function uploadImage(appName: string, event: Event) {
    const file = (event.currentTarget as HTMLInputElement).files?.[0];
    if (!file) {
      return;
    }

    uploading = appName;
    try {
      const formData = new FormData();
      formData.append("json", JSON.stringify({ path: `v1/oauth2/${appName}/_image` }));
      formData.append("image", file);
      const response = await fetch("/api/kanidm", { body: formData, method: "POST" });
      const body = await response.json().catch(() => null) as { body?: unknown; status?: number } | null;
      if (body?.status === 200) {
        addNotification("success", `Uploaded image for ${appName}.`);
        await invalidateAll();
      } else {
        addNotification("error", readableError(body?.body, "Could not upload image."));
      }
    } finally {
      uploading = "";
    }
  }
</script>

<section class="space-y-4">
  <SectionHeader title="OAuth2 applications" description="Manage Kanidm resource servers and the values Better Auth needs for OIDC login.">
    {#snippet action()}
      <button class="button button-primary" type="button" onclick={() => (showCreateForm = !showCreateForm)}>
        <Plus class="size-4" />
        {showCreateForm ? "Close" : "Create"}
      </button>
    {/snippet}
  </SectionHeader>

  {#if showCreateForm}
    <div class="panel p-4">
      <div class="grid gap-4 lg:grid-cols-2">
        <FormField label="Application name">
          <input class="control font-mono" placeholder="manga-server" bind:value={createValues.name} />
        </FormField>
        <FormField label="Display name">
          <input class="control" placeholder="Manga Server" bind:value={createValues.displayName} />
        </FormField>
        <FormField label="Homepage">
          <input class="control font-mono" type="url" placeholder="https://manga.example.com" bind:value={createValues.origin} />
        </FormField>
        <FormField label="Client type">
          <select class="control" bind:value={createValues.type}>
            <option value="basic">Confidential</option>
            <option value="public">Public</option>
          </select>
        </FormField>
        <FormField label="Redirect URLs" class="lg:col-span-2">
          <textarea class="control min-h-24 py-2 font-mono" placeholder="https://manga.example.com/api/auth/callback/oidc" bind:value={createValues.redirectUrls}></textarea>
        </FormField>
      </div>
      <div class="mt-4 flex justify-end gap-2">
        <button class="button button-outline" type="button" onclick={() => (showCreateForm = false)}>Cancel</button>
        <button
          class="button button-primary"
          type="button"
          disabled={!createValues.name || !createValues.displayName || !createValues.origin}
          onclick={createApplication}
        >
          Create application
        </button>
      </div>
    </div>
  {/if}

  <div class="grid gap-4 xl:grid-cols-2">
    {#each apps as app (entryName(app))}
      {@const appName = entryName(app)}
      {@const draft = editing[appName]}
      <article class="panel flex min-h-0 flex-col p-4">
        <div class="flex items-start justify-between gap-3">
          <div class="min-w-0">
            <div class="flex items-center gap-2">
              {#if isPublicClient(app)}
                <Globe2 class="size-4 text-muted-foreground" />
              {:else}
                <LockKeyhole class="size-4 text-primary" />
              {/if}
              <h3 class="truncate text-base font-semibold">{appDisplayName(app)}</h3>
            </div>
            <p class="mt-1 truncate font-mono text-xs text-muted-foreground">{appName}</p>
          </div>
          {#if firstAttr(app, "oauth2_rs_origin_landing")}
            <a class="button button-ghost h-7 px-2" href={firstAttr(app, "oauth2_rs_origin_landing")} target="_blank" rel="noreferrer">
              <ExternalLink class="size-3.5" />
              Visit
            </a>
          {/if}
        </div>

        {#if draft}
          <div class="mt-4 grid gap-3">
            <FormField label="Display name">
              <input class="control" bind:value={draft.displayName} />
            </FormField>
            <FormField label="Homepage">
              <input class="control font-mono" bind:value={draft.origin} />
            </FormField>
            <FormField label="Redirect URLs">
              <textarea class="control min-h-24 py-2 font-mono" bind:value={draft.redirectUrls}></textarea>
            </FormField>
            <div class="grid gap-2 sm:grid-cols-2">
              <CheckboxRow bind:checked={draft.secureCrypto}>
                Secure crypto
              </CheckboxRow>
              <CheckboxRow bind:checked={draft.requirePkce}>
                Require PKCE
              </CheckboxRow>
            </div>
          </div>
        {:else}
          <dl class="mt-4 grid gap-3 text-sm">
            <div class="border border-border bg-background p-3">
              <dt class="muted-label">Homepage</dt>
              <dd class="mt-1 break-all font-mono text-xs">{firstAttr(app, "oauth2_rs_origin_landing", "Not set")}</dd>
            </div>
            <div class="border border-border bg-background p-3">
              <dt class="muted-label">Redirect URLs</dt>
              <dd class="mt-1 space-y-1 font-mono text-xs">
                {#each attr(app, "oauth2_rs_origin") as origin (origin)}
                  <p class="break-all">{origin}</p>
                {:else}
                  <p class="text-muted-foreground">No redirect URLs</p>
                {/each}
              </dd>
            </div>
            <div class="grid gap-3 sm:grid-cols-2">
              <div class="border border-border bg-background p-3">
                <dt class="muted-label">Scope maps</dt>
                <dd class="mt-1 space-y-1 text-xs text-muted-foreground">
                  {#each attr(app, "oauth2_rs_scope_map") as map (map)}
                    <button class="block text-left hover:text-foreground" type="button" title="Remove scope map" onclick={() => deleteScopeMap(appName, map.split(":")[0] ?? "")}>{map}</button>
                  {:else}
                    <span>None</span>
                  {/each}
                </dd>
              </div>
              <div class="border border-border bg-background p-3">
                <dt class="muted-label">Claim maps</dt>
                <dd class="mt-1 text-xs text-muted-foreground">{attr(app, "oauth2_rs_claim_map").length || 0} configured</dd>
              </div>
            </div>
          </dl>
        {/if}

        <div class="mt-4 flex flex-wrap justify-end gap-2 border-t border-border pt-3">
          {#if draft}
            <button class="button button-outline" type="button" onclick={() => cancelEditing(appName)}>Cancel</button>
            <button class="button button-primary" type="button" onclick={() => saveApplication(appName)}>
              <Save class="size-4" />
              Save
            </button>
          {:else}
            <label class="button button-outline cursor-pointer">
              <Upload class="size-4" />
              {uploading === appName ? "Uploading" : "Image"}
              <input class="hidden" type="file" accept="image/*" onchange={(event) => uploadImage(appName, event)} />
            </label>
            {#if !isPublicClient(app)}
              <button class="button button-outline" type="button" onclick={() => copySecret(appName)}>
                <KeyRound class="size-4" />
                Secret
              </button>
            {/if}
            <button class="button button-outline" type="button" onclick={() => startEditing(app)}>Edit</button>
            <button class="button button-danger" type="button" onclick={() => deleteApplication(appName)}>
              <Trash2 class="size-4" />
              Delete
            </button>
          {/if}
        </div>
      </article>
    {:else}
      <EmptyPanel class="xl:col-span-2" message="No OAuth2 applications returned by Kanidm." />
    {/each}
  </div>

  <details class="group border-t border-border/70 pt-4">
    <summary class="flex cursor-pointer list-none items-center justify-between gap-3 text-sm marker:hidden">
      <span>
        <span class="block font-semibold">Advanced OAuth2 mappings</span>
        <span class="mt-1 block text-muted-foreground">Add scope and claim maps when the selected app needs custom group claims.</span>
      </span>
      <ChevronDown class="size-4 shrink-0 text-muted-foreground transition-transform group-open:rotate-180" />
    </summary>

    <div class="mt-4 grid gap-4 lg:grid-cols-2">
      <section class="space-y-3">
        <h3 class="text-sm font-semibold">Add scope map</h3>
        <EntrySelect entries={apps} placeholder="Select application" bind:value={scopeForm.appName} />
        <input class="control" list="kanidm-groups" placeholder="group@domain.example" bind:value={scopeForm.groupName} />
        <input class="control font-mono" bind:value={scopeForm.scopes} />
        <button class="button button-primary ml-auto" type="button" disabled={!scopeForm.appName || !scopeForm.groupName} onclick={addScopeMap}>Add map</button>
      </section>

      <section class="space-y-3">
        <h3 class="text-sm font-semibold">Add claim map</h3>
        <EntrySelect entries={apps} placeholder="Select application" bind:value={claimForm.appName} />
        <div class="grid gap-3 sm:grid-cols-2">
          <input class="control" placeholder="claim name" bind:value={claimForm.claimName} />
          <select class="control" bind:value={claimForm.joinStrategy}>
            <option value="array">Array</option>
            <option value="csv">CSV</option>
            <option value="ssv">SSV</option>
          </select>
        </div>
        <input class="control" list="kanidm-groups" placeholder="group@domain.example" bind:value={claimForm.groupName} />
        <input class="control font-mono" placeholder="name, spn" bind:value={claimForm.claims} />
        <button class="button button-primary ml-auto" type="button" disabled={!claimForm.appName || !claimForm.groupName || !claimForm.claimName} onclick={addClaimMap}>Add claim</button>
      </section>
    </div>
  </details>

  <datalist id="kanidm-groups">
    {#each groups as group (entryName(group))}
      <option value={entryName(group)}>{appDisplayName(group)}</option>
    {/each}
  </datalist>
</section>
