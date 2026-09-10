<script lang="ts">
  import { invalidateAll } from "$app/navigation";
  import { KeyRound, Mail, Plus, Save, Trash2, UserPlus, UserRound, Users } from "@lucide/svelte";
  import EmptyPanel from "@manga-server/ui/components/empty-panel";
  import FormField from "@manga-server/ui/components/form-field";
  import SectionHeader from "@manga-server/ui/components/section-header";
  import type { AddNotification, KanidmEntry } from "$lib/kanidm/types";
  import {
    appDisplayName,
    attr,
    entryName,
    firstAttr,
    kanidmRequest,
    parseCommaList,
    readableError,
    runKanidmMutation,
  } from "$lib/kanidm/utils";
  import EntrySelect from "./EntrySelect.svelte";

  type UserDraft = {
    displayName: string;
    legalName: string;
    mail: string;
  };

  type PasswordDraft = {
    confirm: string;
    value: string;
  };

  type GroupMembership = {
    group: KanidmEntry;
    memberValue: string;
  };

  let {
    addNotification,
    baseUrl,
    groups,
    users,
  }: {
    addNotification: AddNotification;
    baseUrl: string;
    groups: KanidmEntry[];
    users: KanidmEntry[];
  } = $props();

  let showCreateForm = $state(false);
  let search = $state("");
  let editing = $state<Record<string, UserDraft>>({});
  let passwordForms = $state<Record<string, PasswordDraft>>({});
  let createValues = $state({ confirmPassword: "", displayName: "", legalName: "", mail: "", name: "", password: "" });
  let memberForm = $state({ groupName: "", userName: "" });
  let generatedReset = $state<{ url: string; userName: string } | null>(null);
  let resetBusy = $state("");
  let passwordBusy = $state("");

  const filteredUsers = $derived.by(() => {
    const term = search.trim().toLowerCase();
    if (!term) {
      return users;
    }

    return users.filter((user) =>
      [
        entryName(user),
        appDisplayName(user),
        firstAttr(user, "spn"),
        ...attr(user, "mail"),
      ].some((value) => value.toLowerCase().includes(term)),
    );
  });

  function startEditing(user: KanidmEntry) {
    editing[entryName(user)] = {
      displayName: appDisplayName(user),
      legalName: firstAttr(user, "legalname"),
      mail: attr(user, "mail").join(", "),
    };
  }

  async function createUser() {
    const userName = createValues.name.trim().toLowerCase();
    const attrs: Record<string, string[]> = {
      displayname: [createValues.displayName.trim()],
      name: [userName],
    };
    const profileAttrs: Record<string, string[]> = {};
    const legalName = createValues.legalName.trim();
    if (legalName) {
      profileAttrs.legalname = [legalName];
    }
    const mail = parseCommaList(createValues.mail);
    if (mail.length) {
      profileAttrs.mail = mail;
    }

    const response = await runKanidmMutation(fetch, addNotification, {
      body: { attrs },
      method: "POST",
      path: "v1/person",
    }, {
      error: "Could not create user.",
      success: `Created user ${userName}.`,
    });

    if (response) {
      if (Object.keys(profileAttrs).length) {
        await runKanidmMutation(fetch, addNotification, {
          body: { attrs: profileAttrs },
          method: "PATCH",
          path: `v1/person/${userName}`,
        }, {
          error: "Could not update the new user's profile fields.",
          success: `Updated profile fields for ${userName}.`,
        });
      }

      if (createValues.password) {
        await setPasswordForUser(userName, createValues.password, `Set password for ${userName}.`);
      }
      createValues = { confirmPassword: "", displayName: "", legalName: "", mail: "", name: "", password: "" };
      showCreateForm = false;
      await invalidateAll();
    }
  }

  async function saveUser(userName: string) {
    const draft = editing[userName];
    if (!draft) {
      return;
    }

    const response = await runKanidmMutation(fetch, addNotification, {
      body: {
        attrs: {
          displayname: [draft.displayName.trim()],
          legalname: draft.legalName.trim() ? [draft.legalName.trim()] : [],
          mail: parseCommaList(draft.mail),
        },
      },
      method: "PATCH",
      path: `v1/person/${userName}`,
    }, {
      error: "Could not update user.",
      success: `Updated ${userName}.`,
    });

    if (response) {
      delete editing[userName];
      await invalidateAll();
    }
  }

  async function deleteUser(userName: string) {
    if (!confirm(`Delete user ${userName}?`)) {
      return;
    }

    const response = await runKanidmMutation(fetch, addNotification, {
      method: "DELETE",
      path: `v1/person/${userName}`,
    }, {
      error: "Could not delete user.",
      success: `Deleted ${userName}.`,
    });

    if (response) {
      await invalidateAll();
    }
  }

  async function addUserToGroup() {
    const response = await runKanidmMutation(fetch, addNotification, {
      body: [memberForm.userName.trim()],
      method: "POST",
      path: `v1/group/${memberForm.groupName}/_attr/member`,
    }, {
      error: "Could not add user to group.",
      success: `Added ${memberForm.userName} to ${memberForm.groupName}.`,
    });

    if (response) {
      memberForm = { groupName: "", userName: "" };
      await invalidateAll();
    }
  }

  async function removeUserFromGroup(userName: string, groupName: string) {
    const response = await runKanidmMutation(fetch, addNotification, {
      body: [userName],
      method: "DELETE",
      path: `v1/group/${groupName}/_attr/member`,
    }, {
      error: "Could not remove user from group.",
      success: `Removed ${userName} from ${groupName}.`,
    });

    if (response) {
      await invalidateAll();
    }
  }

  function startPasswordChange(userName: string) {
    passwordForms[userName] = { confirm: "", value: "" };
  }

  function cancelPasswordChange(userName: string) {
    delete passwordForms[userName];
  }

  function canSubmitCreate(): boolean {
    return Boolean(createValues.name && createValues.displayName && passwordsMatch(createValues.password, createValues.confirmPassword));
  }

  function canSubmitPassword(draft: PasswordDraft | undefined): boolean {
    return Boolean(draft?.value && passwordsMatch(draft.value, draft.confirm));
  }

  function passwordsMatch(value: string, confirm: string): boolean {
    return value === confirm;
  }

  async function setUserPassword(userName: string) {
    const draft = passwordForms[userName];
    if (!draft || !canSubmitPassword(draft)) {
      return;
    }

    const response = await setPasswordForUser(userName, draft.value, `Set password for ${userName}.`);
    if (response) {
      delete passwordForms[userName];
    }
  }

  async function setPasswordForUser(
    userName: string,
    password: string,
    successMessage: string,
  ): Promise<boolean> {
    passwordBusy = userName;
    try {
      const sessionResponse = await kanidmRequest<unknown>(fetch, {
        method: "GET",
        path: `v1/person/${userName}/_credential/_update`,
      });
      if (sessionResponse.status !== 200) {
        addNotification("error", readableError(sessionResponse.body, "Could not start credential update."));
        return false;
      }

      const sessionToken = credentialSessionToken(sessionResponse.body);
      if (!sessionToken) {
        addNotification("error", "Kanidm did not return a credential update session.");
        return false;
      }

      const updateResponse = await kanidmRequest<unknown>(fetch, {
        body: [{ password }, sessionToken],
        method: "POST",
        path: "v1/credential/_update",
      });
      if (updateResponse.status !== 200) {
        addNotification("error", readableError(updateResponse.body, "Could not update password."));
        return false;
      }

      const commitBlocker = credentialCommitBlocker(updateResponse.body);
      if (commitBlocker) {
        addNotification("error", commitBlocker);
        return false;
      }

      const commitResponse = await kanidmRequest<unknown>(fetch, {
        body: sessionToken,
        method: "POST",
        path: "v1/credential/_commit",
      });
      if (commitResponse.status !== 200) {
        addNotification("error", readableError(commitResponse.body, "Could not commit password update."));
        return false;
      }

      addNotification("success", successMessage);
      return true;
    } finally {
      passwordBusy = "";
    }
  }

  function credentialCommitBlocker(body: unknown): string {
    if (!body || typeof body !== "object") {
      return "";
    }

    const canCommit = (body as Record<string, unknown>).can_commit;
    if (canCommit !== false) {
      return "";
    }

    const warnings = credentialWarnings(body);
    if (warnings.includes("MfaRequired")) {
      return "Kanidm accepted the password, but this account policy requires MFA before credentials can be committed. Generate a reset link for the user to finish setup.";
    }
    if (warnings.includes("PasskeyRequired") || warnings.includes("AttestedPasskeyRequired")) {
      return "Kanidm accepted the password, but this account policy requires a passkey before credentials can be committed. Generate a reset link for the user to finish setup.";
    }

    const details = warnings.length ? ` (${warnings.join(", ")})` : "";
    return `Kanidm accepted the password, but blocked the credential commit by policy${details}.`;
  }

  function credentialWarnings(body: unknown): string[] {
    if (!body || typeof body !== "object") {
      return [];
    }

    const warnings = (body as Record<string, unknown>).warnings;
    if (!Array.isArray(warnings)) {
      return [];
    }

    return warnings.filter((warning): warning is string => typeof warning === "string");
  }

  async function createResetLink(userName: string) {
    resetBusy = userName;
    try {
      const response = await kanidmRequest<unknown>(fetch, {
        method: "GET",
        path: `v1/person/${userName}/_credential/_update_intent/86400`,
      });
      if (response.status !== 200) {
        addNotification("error", readableError(response.body, "Could not create reset link."));
        return;
      }

      const token = credentialResetToken(response.body);
      if (!token) {
        addNotification("error", "Kanidm did not return a credential reset token.");
        return;
      }

      const url = `${baseUrl.replace(/\/+$/, "")}/ui/reset?token=${encodeURIComponent(token)}`;
      generatedReset = { url, userName };
      try {
        await navigator.clipboard.writeText(url);
        addNotification("success", `Copied reset link for ${userName}.`);
      } catch {
        addNotification("info", `Generated reset link for ${userName}.`);
      }
    } finally {
      resetBusy = "";
    }
  }

  function credentialResetToken(body: unknown): string {
    if (typeof body === "string") {
      return body;
    }
    if (!body || typeof body !== "object") {
      return "";
    }

    const object = body as Record<string, unknown>;
    for (const key of ["token", "intent", "intent_token", "cu_intent_token"]) {
      if (typeof object[key] === "string") {
        return object[key];
      }
    }
    return "";
  }

  function credentialSessionToken(body: unknown): { token: string } | null {
    if (!Array.isArray(body)) {
      return null;
    }

    const session = body[0];
    if (!session || typeof session !== "object") {
      return null;
    }

    const token = (session as Record<string, unknown>).token;
    return typeof token === "string" ? { token } : null;
  }

  function groupsForUser(user: KanidmEntry): GroupMembership[] {
    const identifiers = new Set(
      [entryName(user), firstAttr(user, "spn"), firstAttr(user, "uuid")].filter(Boolean),
    );

    return groups.flatMap((group) => {
      const memberValue = attr(group, "member").find((member) => identifiers.has(member));
      return memberValue ? [{ group, memberValue }] : [];
    });
  }
</script>

<section class="space-y-4">
  <SectionHeader title="Users" description="Manage Kanidm person accounts, memberships, and onboarding links.">
    {#snippet action()}
      <button class="button button-primary" type="button" onclick={() => (showCreateForm = !showCreateForm)}>
        <UserPlus class="size-4" />
        {showCreateForm ? "Close" : "Create"}
      </button>
    {/snippet}
  </SectionHeader>

  {#if showCreateForm}
    <div class="panel p-4">
      <div class="grid gap-4 lg:grid-cols-2">
        <FormField label="Username">
          <input class="control font-mono" placeholder="demo_user" bind:value={createValues.name} />
        </FormField>
        <FormField label="Display name">
          <input class="control" placeholder="Demo User" bind:value={createValues.displayName} />
        </FormField>
        <FormField label="Legal name">
          <input class="control" placeholder="Demo User" bind:value={createValues.legalName} />
        </FormField>
        <FormField label="Email addresses">
          <input class="control font-mono" placeholder="demo@example.com" bind:value={createValues.mail} />
        </FormField>
        <FormField label="Password">
          <input class="control font-mono" type="password" autocomplete="new-password" aria-label="Password" bind:value={createValues.password} />
        </FormField>
        <FormField label="Confirm password">
          <input class="control font-mono" type="password" autocomplete="new-password" aria-label="Confirm password" bind:value={createValues.confirmPassword} />
        </FormField>
      </div>
      <div class="mt-4 flex justify-end gap-2">
        <button class="button button-outline" type="button" onclick={() => (showCreateForm = false)}>Cancel</button>
        <button
          class="button button-primary"
          type="button"
          disabled={!canSubmitCreate() || passwordBusy === createValues.name.trim().toLowerCase()}
          onclick={createUser}
        >
          Create user
        </button>
      </div>
    </div>
  {/if}

  {#if generatedReset}
    <div class="panel grid gap-3 p-4 lg:grid-cols-[auto_minmax(0,1fr)_auto] lg:items-center">
      <div class="flex items-center gap-2 text-sm font-medium">
        <KeyRound class="size-4 text-primary" />
        {generatedReset.userName}
      </div>
      <input class="control font-mono" readonly value={generatedReset.url} />
      <a class="button button-outline" href={generatedReset.url} target="_blank" rel="noreferrer">Open</a>
    </div>
  {/if}

  <div class="panel p-4">
    <div class="grid gap-3 lg:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto]">
      <EntrySelect entries={users} placeholder="Select user" bind:value={memberForm.userName} />
      <EntrySelect entries={groups} placeholder="Select group" bind:value={memberForm.groupName} />
      <button class="button button-primary" type="button" disabled={!memberForm.userName || !memberForm.groupName} onclick={addUserToGroup}>
        <Plus class="size-4" />
        Add member
      </button>
    </div>
  </div>

  <input class="control font-mono" placeholder="Search users" bind:value={search} />

  <div class="grid gap-4 xl:grid-cols-2">
    {#each filteredUsers as user (entryName(user))}
      {@const userName = entryName(user)}
      {@const draft = editing[userName]}
      {@const passwordDraft = passwordForms[userName]}
      {@const memberships = groupsForUser(user)}
      <article class="panel p-4">
        <div class="flex items-start justify-between gap-3">
          <div class="min-w-0">
            <div class="flex items-center gap-2">
              <UserRound class="size-4 text-muted-foreground" />
              <h3 class="truncate text-base font-semibold">{appDisplayName(user)}</h3>
            </div>
            <p class="mt-1 truncate font-mono text-xs text-muted-foreground">{firstAttr(user, "spn", userName)}</p>
          </div>
          <div class="flex flex-wrap justify-end gap-2">
            {#if draft}
              <button class="button button-outline h-7 px-2" type="button" onclick={() => delete editing[userName]}>Cancel</button>
              <button class="button button-primary h-7 px-2" type="button" onclick={() => saveUser(userName)}>
                <Save class="size-3.5" />
                Save
              </button>
            {:else if passwordDraft}
              <button class="button button-outline h-7 px-2" type="button" onclick={() => cancelPasswordChange(userName)}>Cancel</button>
              <button class="button button-primary h-7 px-2" type="button" disabled={!canSubmitPassword(passwordDraft) || passwordBusy === userName} onclick={() => setUserPassword(userName)}>
                <Save class="size-3.5" />
                {passwordBusy === userName ? "Saving" : "Save"}
              </button>
            {:else}
              <button class="button button-outline h-7 px-2" type="button" disabled={resetBusy === userName} onclick={() => createResetLink(userName)}>
                <KeyRound class="size-3.5" />
                {resetBusy === userName ? "Creating" : "Reset"}
              </button>
              <button class="button button-outline h-7 px-2" type="button" onclick={() => startPasswordChange(userName)}>
                <KeyRound class="size-3.5" />
                Password
              </button>
              <button class="button button-outline h-7 px-2" type="button" onclick={() => startEditing(user)}>Edit</button>
              <button class="button button-danger h-7 px-2" type="button" onclick={() => deleteUser(userName)}>
                <Trash2 class="size-3.5" />
              </button>
            {/if}
          </div>
        </div>

        {#if draft}
          <div class="mt-4 grid gap-3">
            <FormField label="Display name">
              <input class="control" bind:value={draft.displayName} />
            </FormField>
            <FormField label="Legal name">
              <input class="control" bind:value={draft.legalName} />
            </FormField>
            <FormField label="Email addresses">
              <input class="control font-mono" bind:value={draft.mail} />
            </FormField>
          </div>
        {:else if passwordDraft}
          <div class="mt-4 grid gap-3 sm:grid-cols-2">
            <FormField label="Password">
              <input class="control font-mono" type="password" autocomplete="new-password" aria-label="Password" bind:value={passwordDraft.value} />
            </FormField>
            <FormField label="Confirm password">
              <input class="control font-mono" type="password" autocomplete="new-password" aria-label="Confirm password" bind:value={passwordDraft.confirm} />
            </FormField>
          </div>
        {:else}
          <dl class="mt-4 grid gap-3 text-sm">
            <div class="border border-border bg-background p-3">
              <dt class="muted-label">Email</dt>
              <dd class="mt-1 space-y-1 font-mono text-xs">
                {#each attr(user, "mail") as mail (mail)}
                  <p class="flex items-center gap-2 break-all">
                    <Mail class="size-3.5 text-muted-foreground" />
                    {mail}
                  </p>
                {:else}
                  <p class="text-muted-foreground">Not set</p>
                {/each}
              </dd>
            </div>
            <div class="grid gap-3 sm:grid-cols-2">
              <div class="border border-border bg-background p-3">
                <dt class="muted-label">Legal name</dt>
                <dd class="mt-1 truncate text-xs text-muted-foreground">{firstAttr(user, "legalname", "Not set")}</dd>
              </div>
              <div class="border border-border bg-background p-3">
                <dt class="muted-label">UUID</dt>
                <dd class="mt-1 truncate font-mono text-xs text-muted-foreground">{firstAttr(user, "uuid", "Not set")}</dd>
              </div>
            </div>
          </dl>

          <div class="mt-4 border border-border bg-background p-3">
            <p class="muted-label mb-2">Groups</p>
            <div class="flex flex-wrap gap-2">
              {#each memberships as membership (entryName(membership.group))}
                {@const groupName = entryName(membership.group)}
                <button class="border border-border px-2 py-1 font-mono text-xs text-muted-foreground hover:text-foreground" type="button" onclick={() => removeUserFromGroup(membership.memberValue, groupName)}>
                  <Users class="mr-1 inline size-3.5" />
                  {groupName}
                </button>
              {:else}
                <span class="text-xs text-muted-foreground">No groups</span>
              {/each}
            </div>
          </div>
        {/if}
      </article>
    {:else}
      <EmptyPanel class="xl:col-span-2" message={users.length ? "No users match the search." : "No users returned by Kanidm."} />
    {/each}
  </div>
</section>
