import {
  dehydrate,
  type DehydratedState,
  type QueryClient,
  type QueryKey,
} from "@tanstack/svelte-query";

export function createHydrationState(queryClient: QueryClient) {
  return {
    dehydratedState: dehydrate(queryClient),
  };
}

export function dehydratedQueryOptions<TData>(
  state: DehydratedState | undefined,
  queryKey: QueryKey,
): {
  initialData?: () => TData;
  initialDataUpdatedAt?: number;
  refetchOnMount: false;
} {
  const dehydratedQuery = state?.queries.find((query) => sameQueryKey(query.queryKey, queryKey));

  if (!dehydratedQuery || dehydratedQuery.state.status !== "success") {
    return { refetchOnMount: false };
  }

  return {
    initialData: () => dehydratedQuery.state.data as TData,
    initialDataUpdatedAt: dehydratedQuery.state.dataUpdatedAt,
    refetchOnMount: false,
  };
}

function sameQueryKey(left: QueryKey, right: QueryKey): boolean {
  return JSON.stringify(left) === JSON.stringify(right);
}
