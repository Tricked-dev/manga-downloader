<script lang="ts">
  import PageHeader from "@manga-server/ui/components/page-header";
  import AboutCapabilitiesCard from "$components/features/about/AboutCapabilitiesCard.svelte";
  import AboutDeploymentCard from "$components/features/about/AboutDeploymentCard.svelte";
  import AboutHeroStatus from "$components/features/about/AboutHeroStatus.svelte";
  import AboutLinksCard from "$components/features/about/AboutLinksCard.svelte";
  import AboutRuntimeCard from "$components/features/about/AboutRuntimeCard.svelte";
  import type { CapabilityItem, DetailItem, HealthCheckItem, LinkItem, StatusItem, StatusTone } from "$lib/features/about/types";
  import { locale, t } from "$lib/i18n";

  const { data } = $props<{
    data: {
      application: {
        buildChannel: string | null;
        buildTime: string | null;
        commitShortHash: string | null;
        generatedAt: string;
        name: string;
        version: string;
      };
      links: {
        repository: string | null;
      };
      server: {
        branch: string | null;
        buildChannel: string | null;
        buildTarget: string | null;
        buildTime: string | null;
        commitShortHash: string | null;
        healthChecks: HealthCheckItem[];
        healthy: boolean;
        gitClean: boolean | null;
        isLatestCommit: boolean | null;
        lastCommitAt: string | null;
        lastCommitMessage: string | null;
        name: string;
        remoteBranch: string | null;
        remoteCommitShortHash: string | null;
        version: string;
      };
    };
  }>();

  const dateTimeFormatter = $derived(createDateTimeFormatter($locale));

  function formatDateTime(value: string | null): string | null {
    if (!value) {
      return null;
    }

    const parsed = new Date(value);
    return Number.isNaN(parsed.getTime()) ? value : dateTimeFormatter.format(parsed);
  }

  function healthSummary(checks: HealthCheckItem[]): string {
    if (checks.length === 0) {
      return $t("app.about.health.responding");
    }

    const healthyCount = checks.filter((check) => check.ok).length;
    return $t("app.about.health.componentsHealthy", {
      healthy: healthyCount,
      total: checks.length,
    });
  }

  const frontendDetails = $derived(getFrontendDetails());
  const serverDetails = $derived(getServerDetails());
  const deploymentDetails = $derived(getDeploymentDetails());
  const capabilityItems = $derived(getCapabilityItems());
  const linkItems = $derived(getLinkItems());
  const backendStatus = $derived(getBackendStatus());
  const gitStatus = $derived(getGitStatus());
  const releaseStatus = $derived(getReleaseStatus());
  const statusItems: StatusItem[] = $derived(getStatusItems(backendStatus, gitStatus, releaseStatus));

  function createDateTimeFormatter(currentLocale: string) {
    return new Intl.DateTimeFormat(currentLocale || "en-US", {
      dateStyle: "medium",
      timeStyle: "short",
      timeZone: "UTC",
    });
  }

  function populatedDetails(items: DetailItem[]) {
    return items.filter((item) => item.value.trim().length > 0 || Boolean(item.detail));
  }

  function getFrontendDetails(): DetailItem[] {
    return populatedDetails([
      { label: $t("app.about.details.version"), value: `v${data.application.version}` },
      { label: $t("app.about.details.commit"), mono: true, value: data.application.commitShortHash ?? "" },
      { label: $t("app.about.details.buildProfile"), value: data.application.buildChannel ?? "" },
      { label: $t("app.about.details.buildGenerated"), value: formatDateTime(data.application.generatedAt) ?? "" },
      { label: $t("app.about.details.builtAt"), value: formatDateTime(data.application.buildTime) ?? "" },
    ]);
  }

  function getServerDetails(): DetailItem[] {
    return populatedDetails([
      { label: $t("app.about.details.version"), value: `v${data.server.version}` },
      { label: $t("app.about.details.service"), mono: true, value: data.server.name },
      { label: $t("app.about.details.commit"), mono: true, value: data.server.commitShortHash ?? "" },
      { label: $t("app.about.details.buildProfile"), value: data.server.buildChannel ?? "" },
      { label: $t("app.about.details.target"), mono: true, value: data.server.buildTarget ?? "" },
      { label: $t("app.about.details.builtAt"), value: formatDateTime(data.server.buildTime) ?? "" },
    ]);
  }

  function getDeploymentDetails(): DetailItem[] {
    const details: DetailItem[] = [
      { icon: "branch", label: $t("app.about.details.branch"), mono: true, value: data.server.branch ?? "" },
      { icon: "branch", label: $t("app.about.details.upstream"), mono: true, value: data.server.remoteBranch ?? "" },
      { icon: "remoteCommit", label: $t("app.about.details.remoteCommit"), mono: true, value: data.server.remoteCommitShortHash ?? "" },
    ];

    if (data.server.lastCommitMessage) {
      details.push({
        icon: "lastCommit",
        label: $t("app.about.details.lastCommit"),
        detail: formatDateTime(data.server.lastCommitAt) ?? undefined,
        value: data.server.lastCommitMessage,
      });
    }

    return populatedDetails(details);
  }

  function getCapabilityItems(): CapabilityItem[] {
    return [
      {
        title: $t("app.about.capabilities.libraryFirst.title"),
        description: $t("app.about.capabilities.libraryFirst.description"),
      },
      {
        title: $t("app.about.capabilities.sources.title"),
        description: $t("app.about.capabilities.sources.description"),
      },
      {
        title: $t("app.about.capabilities.readerAware.title"),
        description: $t("app.about.capabilities.readerAware.description"),
      },
      {
        title: $t("app.about.capabilities.clientPackages.title"),
        description: $t("app.about.capabilities.clientPackages.description"),
      },
    ];
  }

  function getLinkItems(): LinkItem[] {
    const items: LinkItem[] = [
      {
        href: data.links.repository ?? "",
        id: "repository",
        label: $t("app.about.links.repository"),
        value: data.links.repository ?? "",
      },
    ];

    return items.filter((item) => item.href.length > 0 && item.value.length > 0);
  }

  function getBackendStatus(): StatusItem {
    if (data.server.healthy) {
      return {
        checks: data.server.healthChecks,
        detail: healthSummary(data.server.healthChecks),
        id: "server",
        label: $t("app.about.status.server"),
        tone: "ok",
        value: $t("app.about.status.online"),
      };
    }

    return {
      checks: data.server.healthChecks,
      detail: data.server.healthChecks.length > 0
        ? healthSummary(data.server.healthChecks)
        : $t("app.about.health.unavailable"),
      id: "server",
      label: $t("app.about.status.server"),
      tone: "warn",
      value: $t("app.about.status.offline"),
    };
  }

  function getGitStatus(): StatusItem {
    if (data.server.gitClean === true) {
      return { detail: $t("app.about.status.gitCleanDetail"), id: "git", label: $t("app.about.status.gitTree"), tone: "ok", value: $t("app.about.status.clean") };
    }
    if (data.server.gitClean === false) {
      return { detail: $t("app.about.status.gitDirtyDetail"), id: "git", label: $t("app.about.status.gitTree"), tone: "warn", value: $t("app.about.status.dirty") };
    }

    return { detail: $t("app.about.status.gitUnknownDetail"), id: "git", label: $t("app.about.status.gitTree"), tone: "muted", value: $t("app.about.status.unknown") };
  }

  function getReleaseStatus(): StatusItem {
    if (data.server.isLatestCommit === true) {
      return { detail: $t("app.about.status.upstreamCurrentDetail"), id: "upstream", label: $t("app.about.status.upstream"), tone: "ok", value: $t("app.about.status.current") };
    }
    if (data.server.isLatestCommit === false) {
      return { detail: $t("app.about.status.upstreamBehindDetail"), id: "upstream", label: $t("app.about.status.upstream"), tone: "warn", value: $t("app.about.status.behind") };
    }

    return { detail: $t("app.about.status.upstreamNotComparedDetail"), id: "upstream", label: $t("app.about.status.upstream"), tone: "muted", value: $t("app.about.status.notCompared") };
  }

  function getStatusItems(
    currentBackendStatus: StatusItem,
    currentGitStatus: StatusItem,
    currentReleaseStatus: StatusItem,
  ): StatusItem[] {
    return [currentBackendStatus, currentGitStatus, currentReleaseStatus];
  }
</script>

<svelte:head>
  <title>{$t("app.about.title")} | {$t("app.appLogo.title")}</title>
</svelte:head>

<div class="max-w-6xl space-y-5">
  <PageHeader
    title={$t("app.about.title")}
    description={$t("app.about.description")}
    innerClass="gap-4 lg:items-center"
    descriptionClass="max-w-3xl"
  />

  <AboutHeroStatus
    appName={data.application.name}
    appVersion={data.application.version}
    serverVersion={data.server.version}
    {statusItems}
  />

  <AboutCapabilitiesCard items={capabilityItems} />

  <section class="grid items-start gap-4 2xl:grid-cols-[minmax(0,1.35fr)_minmax(26rem,1fr)]">
    <AboutRuntimeCard {frontendDetails} {serverDetails} />

    <div class="grid gap-4">
      <AboutDeploymentCard items={deploymentDetails} />
      <AboutLinksCard items={linkItems} />
    </div>
  </section>

</div>
