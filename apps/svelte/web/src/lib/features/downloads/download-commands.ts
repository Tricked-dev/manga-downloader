import { getGetDownloadsQueryKey } from "@manga-server/api-client/generated";

interface Mutation<TVariables> {
  mutateAsync(variables: TVariables): Promise<unknown>;
}

interface QueryClientLike {
  invalidateQueries(options: { queryKey: readonly unknown[] }): Promise<unknown>;
}

export async function invalidateDownloads(queryClient: QueryClientLike) {
  await queryClient.invalidateQueries({ queryKey: getGetDownloadsQueryKey() });
}

export async function clearFailedDownloads(mutation: Mutation<void>, queryClient: QueryClientLike) {
  await mutation.mutateAsync(undefined);
  await invalidateDownloads(queryClient);
}

export async function removeDownloads(
  mutation: Mutation<{ id: string }>,
  queryClient: QueryClientLike,
  ids: readonly string[],
  removingIds: Set<string>,
) {
  for (const id of ids) {
    removingIds.add(id);
  }

  try {
    for (const id of ids) {
      await mutation.mutateAsync({ id });
    }
    await invalidateDownloads(queryClient);
  } finally {
    for (const id of ids) {
      removingIds.delete(id);
    }
  }
}
