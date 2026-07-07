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
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, [queryClient]);
}
