<script lang="ts">
  import { Ban, CheckCircle2, ChevronLeft, ChevronRight, Clock, Search, User } from "@lucide/svelte";
  import { Badge } from "$lib/ui/badge";
  import { Button } from "$lib/ui/button";
  import { Input } from "$lib/ui/input";
  import { t } from "$lib/i18n";
  import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "$lib/ui/table";
  import type { SessionUser } from "./dev-api";

  const {
    authEnabled,
    isAuthenticated,
    nextUserPage,
    previousUserPage,
    searchQuery,
    totalUserPages,
    userPage,
    visibleUser,
    onSearchInput,
  }: {
    authEnabled: boolean;
    isAuthenticated: boolean;
    nextUserPage: () => void;
    previousUserPage: () => void;
    searchQuery: string;
    totalUserPages: number;
    userPage: number;
    visibleUser: SessionUser | null;
    onSearchInput: (event: Event) => void;
  } = $props();

  function userInitials(value: SessionUser): string {
    const label = value.name ?? value.email ?? "?";
    return label
      .split(/\s+/)
      .map((part) => part[0])
      .join("")
      .toUpperCase()
      .slice(0, 2);
  }
</script>

<div class="space-y-4">
  <div class="flex flex-col justify-between gap-3 border border-border bg-card p-4 sm:flex-row sm:items-center">
    <div class="relative w-full sm:max-w-md">
      <Search class="absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
      <Input placeholder={$t("app.dev.admin.searchSession")} class="pl-9" value={searchQuery} oninput={onSearchInput} />
    </div>
    <Badge variant={isAuthenticated ? "success" : "outline"} class="h-6">
      {isAuthenticated ? $t("app.dev.admin.authenticated") : authEnabled ? $t("app.dev.toolbar.guest") : $t("app.dev.toolbar.authDisabled")}
    </Badge>
  </div>

  <div class="overflow-hidden border border-border bg-card">
    <Table>
      <TableHeader>
        <TableRow class="hover:bg-transparent">
          <TableHead>{$t("app.dev.admin.user")}</TableHead>
          <TableHead>{$t("app.dev.admin.role")}</TableHead>
          <TableHead>{$t("app.dev.admin.status")}</TableHead>
          <TableHead>{$t("app.dev.admin.source")}</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {#if visibleUser}
          <TableRow>
            <TableCell>
              <div class="flex items-center gap-3">
                <div class="flex size-9 items-center justify-center rounded-full border border-border bg-muted text-xs font-bold">
                  {userInitials(visibleUser)}
                </div>
                <div class="min-w-0">
                  <p class="truncate text-sm font-medium">{visibleUser.name ?? $t("app.dev.admin.noName")}</p>
                  <p class="truncate text-xs text-muted-foreground">{visibleUser.email ?? $t("app.dev.admin.noEmail")}</p>
                </div>
              </div>
            </TableCell>
            <TableCell><Badge variant={visibleUser.role === "admin" ? "destructive" : "secondary"}>{visibleUser.role ?? "user"}</Badge></TableCell>
            <TableCell>
              <Badge variant="success" class="gap-1">
                <CheckCircle2 class="size-3" />
                {$t("app.dev.admin.active")}
              </Badge>
            </TableCell>
            <TableCell class="text-sm text-muted-foreground">{$t("app.dev.admin.currentSession")}</TableCell>
          </TableRow>
        {:else}
          <TableRow>
            <td colspan="4" class="h-44 p-3 text-center align-middle text-muted-foreground">
              <div class="flex flex-col items-center gap-2">
                {#if authEnabled}
                  <User class="size-6 opacity-40" />
                  <span>{isAuthenticated ? $t("app.dev.admin.noUsersMatch") : $t("app.dev.admin.signInInspect")}</span>
                {:else}
                  <Ban class="size-6 opacity-40" />
                  <span>{$t("app.dev.admin.authDisabledDescription")}</span>
                {/if}
              </div>
            </td>
          </TableRow>
        {/if}
      </TableBody>
    </Table>
  </div>

  <div class="flex items-center justify-between border-t border-border pt-4">
    <div class="flex items-center gap-2 text-xs text-muted-foreground">
      <Clock class="size-3.5" />
      {$t("app.dev.admin.userAdminDisabled")}
    </div>
    <div class="flex items-center gap-1">
      <Button variant="outline" size="icon" disabled={userPage === 1} onclick={previousUserPage}>
        <ChevronLeft class="size-4" />
      </Button>
      <span class="min-w-12 text-center text-xs font-medium">{userPage} / {totalUserPages}</span>
      <Button variant="outline" size="icon" disabled={userPage >= totalUserPages} onclick={nextUserPage}>
        <ChevronRight class="size-4" />
      </Button>
    </div>
  </div>
</div>
