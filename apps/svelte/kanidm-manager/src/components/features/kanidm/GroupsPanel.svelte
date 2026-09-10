<script lang="ts">
  import { invalidateAll } from "$app/navigation";
  import { Plus, Save, Trash2, Users } from "@lucide/svelte";
  import EmptyPanel from "@manga-server/ui/components/empty-panel";
  import SectionHeader from "@manga-server/ui/components/section-header";
  import type { AddNotification, KanidmEntry } from "$lib/kanidm/types";
  import { appDisplayName, attr, entryName, firstAttr, runKanidmMutation } from "$lib/kanidm/utils";
  import EntrySelect from "./EntrySelect.svelte";

  let {
    groups,
    addNotification,
  }: {
    addNotification: AddNotification;
    groups: KanidmEntry[];
  } = $props();

  let showCreateForm = $state(false);
  let editing = $state<Record<string, { description: string; displayName: string }>>({});
  let createValues = $state({ description: "", displayName: "", name: "" });
  let memberForm = $state({ groupName: "", memberName: "" });

  function startEditing(group: KanidmEntry) {
    editing[entryName(group)] = {
      description: firstAttr(group, "description"),
      displayName: appDisplayName(group),
    };
  }

  async function createGroup() {
    const response = await runKanidmMutation(fetch, addNotification, {
      body: {
        attrs: {
          description: createValues.description.trim() ? [createValues.description.trim()] : [],
          displayname: [createValues.displayName.trim()],
          name: [createValues.name.trim().toLowerCase()],
        },
      },
      method: "POST",
      path: "v1/group",
    }, {
      error: "Could not create group.",
      success: `Created group ${createValues.name}.`,
    });
    if (response) {
      createValues = { description: "", displayName: "", name: "" };
      showCreateForm = false;
      await invalidateAll();
    }
  }

  async function saveGroup(groupName: string) {
    const draft = editing[groupName];
    if (!draft) {
      return;
    }
    const response = await runKanidmMutation(fetch, addNotification, {
      body: {
        attrs: {
          description: draft.description.trim() ? [draft.description.trim()] : [],
          displayname: [draft.displayName.trim()],
        },
      },
      method: "PATCH",
      path: `v1/group/${groupName}`,
    }, {
      error: "Could not update group.",
      success: `Updated ${groupName}.`,
    });
    if (response) {
      delete editing[groupName];
      await invalidateAll();
    }
  }

  async function deleteGroup(groupName: string) {
    if (!confirm(`Delete group ${groupName}?`)) {
      return;
    }
    const response = await runKanidmMutation(fetch, addNotification, { method: "DELETE", path: `v1/group/${groupName}` }, {
      error: "Could not delete group.",
      success: `Deleted ${groupName}.`,
    });
    if (response) {
      await invalidateAll();
    }
  }

  async function addMember() {
    const response = await runKanidmMutation(fetch, addNotification, {
      body: [memberForm.memberName.trim()],
      method: "POST",
      path: `v1/group/${memberForm.groupName}/_attr/member`,
    }, {
      error: "Could not add group member.",
      success: `Added member to ${memberForm.groupName}.`,
    });
    if (response) {
      memberForm.memberName = "";
      await invalidateAll();
    }
  }

  async function removeMember(groupName: string, memberName: string) {
    const response = await runKanidmMutation(fetch, addNotification, {
      body: [memberName],
      method: "DELETE",
      path: `v1/group/${groupName}/_attr/member`,
    }, {
      error: "Could not remove group member.",
      success: `Removed member from ${groupName}.`,
    });
    if (response) {
      await invalidateAll();
    }
  }
</script>

<section class="space-y-4">
  <SectionHeader title="Groups" description="Manage groups used by OAuth2 scope and claim maps.">
    {#snippet action()}
      <button class="button button-primary" type="button" onclick={() => (showCreateForm = !showCreateForm)}>
        <Plus class="size-4" />
        {showCreateForm ? "Close" : "Create"}
      </button>
    {/snippet}
  </SectionHeader>

  {#if showCreateForm}
    <div class="panel grid gap-3 p-4 lg:grid-cols-3">
      <input class="control font-mono" placeholder="group name" bind:value={createValues.name} />
      <input class="control" placeholder="Display name" bind:value={createValues.displayName} />
      <input class="control" placeholder="Description" bind:value={createValues.description} />
      <button
        class="button button-primary justify-self-end lg:col-span-3"
        type="button"
        disabled={!createValues.name || !createValues.displayName}
        onclick={createGroup}
      >
        Create group
      </button>
    </div>
  {/if}

  <div class="grid gap-4 xl:grid-cols-2">
    {#each groups as group (entryName(group))}
      {@const groupName = entryName(group)}
      {@const draft = editing[groupName]}
      <article class="panel p-4">
        <div class="flex items-start justify-between gap-3">
          <div class="min-w-0">
            <div class="flex items-center gap-2">
              <Users class="size-4 text-muted-foreground" />
              <h3 class="truncate text-base font-semibold">{appDisplayName(group)}</h3>
            </div>
            <p class="mt-1 truncate font-mono text-xs text-muted-foreground">{groupName}</p>
          </div>
          <div class="flex gap-2">
            {#if draft}
              <button class="button button-outline h-7 px-2" type="button" onclick={() => delete editing[groupName]}>Cancel</button>
              <button class="button button-primary h-7 px-2" type="button" onclick={() => saveGroup(groupName)}>
                <Save class="size-3.5" />
                Save
              </button>
            {:else}
              <button class="button button-outline h-7 px-2" type="button" onclick={() => startEditing(group)}>Edit</button>
              <button class="button button-danger h-7 px-2" type="button" onclick={() => deleteGroup(groupName)}>
                <Trash2 class="size-3.5" />
              </button>
            {/if}
          </div>
        </div>

        {#if draft}
          <div class="mt-4 grid gap-3">
            <input class="control" bind:value={draft.displayName} />
            <input class="control" bind:value={draft.description} />
          </div>
        {:else}
          <p class="mt-3 text-sm text-muted-foreground">{firstAttr(group, "description", "No description")}</p>
          <div class="mt-4 border border-border bg-background p-3">
            <p class="muted-label mb-2">Members</p>
            <div class="flex flex-wrap gap-2">
              {#each attr(group, "member") as member (member)}
                <button class="border border-border px-2 py-1 font-mono text-xs text-muted-foreground hover:text-foreground" type="button" onclick={() => removeMember(groupName, member)}>
                  {member}
                </button>
              {:else}
                <span class="text-xs text-muted-foreground">No members</span>
              {/each}
            </div>
          </div>
        {/if}
      </article>
    {:else}
      <EmptyPanel class="xl:col-span-2" message="No groups returned by Kanidm." />
    {/each}
  </div>

  <div class="panel p-4">
    <h3 class="text-sm font-semibold">Add group member</h3>
    <div class="mt-3 grid gap-3 lg:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto]">
      <EntrySelect entries={groups} placeholder="Select group" bind:value={memberForm.groupName} />
      <input class="control font-mono" placeholder="person or group name" bind:value={memberForm.memberName} />
      <button class="button button-primary" type="button" disabled={!memberForm.groupName || !memberForm.memberName} onclick={addMember}>Add member</button>
    </div>
  </div>
</section>
