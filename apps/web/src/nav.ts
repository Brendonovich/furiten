import { useLocation, useNavigate } from "@solidjs/router";

/** History state on an entry that opened an overlay (a sheet, a review) over the page before it. */
type Opened = { back?: boolean };

/**
 * Opening and closing overlays as URLs. `open` pushes an entry marked as opened in the app; `close` goes back to the
 * page under it when it was, so Back and Cancel agree, and otherwise (a reload, a shared link) replaces it with `to`.
 */
export function useOverlay() {
  const location = useLocation<Opened>();
  const navigate = useNavigate();
  return {
    open: (to: string) => navigate(to, { state: { back: true } }),
    /** Moves within an overlay (a sheet's next step) without adding a history entry. */
    replace: (to: string) => navigate(to, { replace: true, state: location.state ?? undefined }),
    close: (to: string) => (location.state?.back ? history.back() : navigate(to, { replace: true })),
    /** The current URL with `params` set (or removed, when empty). */
    with: (params: Record<string, string | number | null | undefined>) => {
      const q = new URLSearchParams(location.search);
      for (const [k, v] of Object.entries(params)) {
        if (v === undefined || v === null || v === "") q.delete(k);
        else q.set(k, String(v));
      }
      const s = q.toString();
      return `${location.pathname}${s ? `?${s}` : ""}`;
    },
  };
}
