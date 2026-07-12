import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { useQueryClient } from "@tanstack/react-query";

/**
 * Subscribes once to the backend's coarse `data-changed` event and invalidates the
 * relevant React Query caches. Deliberately decoupled from the event payload shape —
 * "something changed" triggers a refetch through the normal command, so schema/query
 * changes on the Rust side never require keeping an event payload type in sync in two
 * languages.
 */
export function useDataChangedEvents() {
  const queryClient = useQueryClient();

  useEffect(() => {
    const unlisten = listen("data-changed", () => {
      queryClient.invalidateQueries({ queryKey: ["projects"] });
      queryClient.invalidateQueries({ queryKey: ["sessions"] });
      queryClient.invalidateQueries({ queryKey: ["dashboard"] });
      // Prefix match: invalidates every ["board", projectId] entry, so an open board picks
      // up auto-created cards and auto-sync column moves as sessions start/end.
      queryClient.invalidateQueries({ queryKey: ["board"] });
      // Prefix match: invalidates every ["session-detail", id] entry regardless of which
      // session it's for, so an open SessionDetailModal picks up a just-finished summary,
      // tags, or finalized cost without the user having to close and reopen it.
      queryClient.invalidateQueries({ queryKey: ["session-detail"] });
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, [queryClient]);
}
