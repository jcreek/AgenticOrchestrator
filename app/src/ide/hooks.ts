import { type RefObject, useEffect, useState } from "react";

/** True while the element is displayed and the window is not minimised or hidden. */
export function useVisible(ref: RefObject<HTMLElement | null>): boolean {
  const [shown, setShown] = useState(false);
  const [pageVisible, setPageVisible] = useState(() => !document.hidden);

  useEffect(() => {
    const element = ref.current;
    if (!element) return;
    const observer = new IntersectionObserver((entries) => setShown(entries[entries.length - 1].isIntersecting));
    observer.observe(element);
    return () => observer.disconnect();
  }, [ref]);

  useEffect(() => {
    const update = () => setPageVisible(!document.hidden);
    document.addEventListener("visibilitychange", update);
    return () => document.removeEventListener("visibilitychange", update);
  }, []);

  return shown && pageVisible;
}

/** Runs `task` now and then every `intervalMs` after it settles, while `enabled`; restarts when `task` changes. */
export function usePoll(task: () => Promise<unknown>, intervalMs: number, enabled: boolean) {
  useEffect(() => {
    if (!enabled) return;
    let stopped = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const tick = async () => {
      try {
        await task();
      } catch {
        // Each task reports its own errors in its state.
      }
      if (!stopped) timer = setTimeout(tick, intervalMs);
    };
    void tick();
    return () => {
      stopped = true;
      clearTimeout(timer);
    };
  }, [task, enabled, intervalMs]);
}

export type MonacoModule = typeof import("./monaco");

/** Loads the Monaco chunk the first time `enabled` is true. */
export function useMonaco(enabled: boolean): { monaco: MonacoModule | null; failed: string | null } {
  const [monaco, setMonaco] = useState<MonacoModule | null>(null);
  const [failed, setFailed] = useState<string | null>(null);
  useEffect(() => {
    if (!enabled || monaco) return;
    let live = true;
    import("./monaco").then(
      (module) => live && setMonaco(module),
      (error: unknown) => live && setFailed(String(error)),
    );
    return () => {
      live = false;
    };
  }, [enabled, monaco]);
  return { monaco, failed };
}
