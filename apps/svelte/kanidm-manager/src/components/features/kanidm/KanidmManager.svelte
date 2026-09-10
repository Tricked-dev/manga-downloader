<script lang="ts">
  import { ExternalLink, KeyRound, Shield, UserRound, Users } from "@lucide/svelte";
  import PageHeader from "@manga-server/ui/components/page-header";
  import StatusBanner from "$components/common/StatusBanner.svelte";
  import Toaster from "$components/common/Toaster.svelte";
  import KanidmShell from "$components/layout/KanidmShell.svelte";
  import type {
    KanidmManagerData,
    KanidmManagerTab,
    Notification,
    NotificationType,
  } from "$lib/kanidm/types";
  import BetterAuthBridge from "./BetterAuthBridge.svelte";
  import GroupsPanel from "./GroupsPanel.svelte";
  import OauthApplications from "./OauthApplications.svelte";
  import UsersPanel from "./UsersPanel.svelte";

  let { data } = $props<{ data: KanidmManagerData }>();

  let activeTab = $state<KanidmManagerTab>("oauth2");
  let notifications = $state<Notification[]>([]);

  const navItems = $derived([
    {
      count: data.apps.body.length,
      icon: Shield,
      id: "oauth2" as const,
      label: "OAuth2",
    },
    {
      count: data.users.body.length,
      icon: UserRound,
      id: "users" as const,
      label: "Users",
    },
    {
      count: data.groups.body.length,
      icon: Users,
      id: "groups" as const,
      label: "Groups",
    },
    {
      icon: KeyRound,
      id: "better-auth" as const,
      label: "Better Auth",
    },
  ]);

  function addNotification(type: NotificationType, message: string) {
    const id = crypto.randomUUID();
    notifications = [{ id, message, type }, ...notifications].slice(0, 4);
    setTimeout(() => removeNotification(id), 5000);
  }

  function removeNotification(id: string) {
    notifications = notifications.filter((notification) => notification.id !== id);
  }

  function selectTab(tab: KanidmManagerTab) {
    activeTab = tab;
  }
</script>

<Toaster {notifications} {removeNotification} />

<KanidmShell activeTab={activeTab} {navItems} onSelect={selectTab}>
  <div class="space-y-6">
    <PageHeader
      eyebrow="Manga Server"
      title="Kanidm Manager"
      description="Manage identity records, OAuth2 clients, and the Better Auth settings used by the reader frontend."
    >
      {#snippet actions()}
        {#if data.kanidm.baseUrl}
          <a class="button button-outline" href={data.kanidm.baseUrl} target="_blank" rel="noreferrer">
            <ExternalLink class="size-4" />
            Kanidm console
          </a>
        {/if}
      {/snippet}
    </PageHeader>

    <section class="grid gap-2 lg:grid-cols-2" aria-label="Connection status">
      <StatusBanner
        configured={data.kanidm.configured}
        label={data.kanidm.configured ? `Kanidm proxy: ${data.kanidm.baseUrl}` : "Kanidm proxy is not configured"}
        missing="Set KANIDM_BASE_URL, KANIDM_USERNAME, and KANIDM_PASSWORD for this app."
      />
      <StatusBanner
        configured={data.mangaSettings.configured}
        label={data.mangaSettings.configured ? `Manga settings: ${data.mangaSettings.baseUrl}` : "Manga settings sync is not configured"}
        missing="Set MANGA_SERVER_BASE_URL and, when needed, MANGA_SERVER_API_KEY."
      />
    </section>

    <section id="kanidm-content" role="tabpanel" tabindex="0" class="outline-none">
      {#if activeTab === "oauth2"}
        <OauthApplications apps={data.apps.body} groups={data.groups.body} {addNotification} />
      {:else if activeTab === "users"}
        <UsersPanel users={data.users.body} groups={data.groups.body} baseUrl={data.kanidm.baseUrl} {addNotification} />
      {:else if activeTab === "groups"}
        <GroupsPanel groups={data.groups.body} {addNotification} />
      {:else}
        <BetterAuthBridge
          apps={data.apps.body}
          configured={data.mangaSettings.configured}
          origin={data.origin}
          settings={data.mangaSettingsState.settings}
          {addNotification}
        />
      {/if}
    </section>
  </div>
</KanidmShell>
