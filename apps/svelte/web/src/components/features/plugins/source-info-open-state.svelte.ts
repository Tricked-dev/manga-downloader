import type { Source } from "$lib/types";

export interface SourceInfoOpenState {
  isOpen: (sourceName: string) => boolean;
  setOpen: (source: Source, open: boolean) => void;
}

export function createSourceInfoOpenState(onOpenInfo: (source: Source) => void | Promise<void>) {
  let openSourceInfoName = $state<string | null>(null);

  function isOpen(sourceName: string) {
    return openSourceInfoName === sourceName;
  }

  function setOpen(source: Source, open: boolean) {
    if (!open) {
      if (isOpen(source.name)) {
        openSourceInfoName = null;
      }
      return;
    }

    openSourceInfoName = source.name;
    void onOpenInfo(source);
  }

  return { isOpen, setOpen } satisfies SourceInfoOpenState;
}
