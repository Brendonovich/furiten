import { Drawer } from "@kobalte/core/drawer";
import { createEffect, createMemo, createSignal, onSettled } from "solid-js";
import type { Element } from "solid-js";

/**
 * Dismisses a sheet with its exit animation, from anywhere in it: a Cancel button, after saving. Make one with
 * `createSheet()`, pass it to `<Sheet sheet={…}>`, and call `dismiss()` instead of closing directly.
 */
export type SheetHandle = { dismiss: () => void; bind: (dismiss: () => void) => void };

export function createSheet(): SheetHandle {
  let bound: (() => void) | null = null;
  return {
    dismiss: () => bound?.(),
    bind: (dismiss) => {
      bound = dismiss;
    },
  };
}

/**
 * A bottom drawer (Kobalte's, whose drag and snap maths come from Corvu): drag it down to dismiss, with velocity
 * and scroll handling; tapping outside or Escape dismiss it too. It rises when mounted, and every dismissal slides
 * it away before `onClose` runs, so the owner can unmount it then.
 */
export function Sheet(props: {
  title: string;
  onClose: () => void;
  children: Element;
  tall?: boolean;
  sheet?: SheetHandle;
  /** What to focus once it's open; otherwise the sheet itself, so no control shows as focused. */
  initialFocus?: () => HTMLElement | undefined;
}) {
  // Mounted closed, then opened, so the drawer plays its opening transition.
  const [open, setOpen] = createSignal(false);
  let content: HTMLElement | undefined;
  const target = () => props.initialFocus?.() ?? content;
  const dismiss = () => {
    // A focused field would keep the keyboard up while the sheet leaves.
    (document.activeElement as HTMLElement | null)?.blur?.();
    setOpen(false);
  };
  props.sheet?.bind(dismiss);
  onSettled(() => {
    requestAnimationFrame(() => {
      setOpen(true);
      // Kobalte's Drawer.Content drops `onOpenAutoFocus`, so its focus trap always focuses the first button (a
      // Cancel button, ringed as if tabbed to). Focus the requested element or the sheet instead, and take it back
      // if the trap moves focus in the first half second. (The target may not exist yet, so it's looked up each time.)
      const focus = () => {
        const el = target();
        if (el?.isConnected && document.activeElement !== el) el.focus({ preventScroll: true });
      };
      document.addEventListener("focusin", focus);
      setTimeout(() => document.removeEventListener("focusin", focus), 500);
      focus();
    });
    // Kobalte 2's alpha (built for Solid rc.16) never registers its dismissable layer on rc.20, so it ignores
    // Escape and outside taps; handle both here until it catches up.
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && dismiss();
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  });

  return (
    <Drawer
      open={open()}
      onOpenChange={(next) => (next ? setOpen(true) : dismiss())}
      side="bottom"
      breakPoints={[0.75]}
    >
      <Drawer.Portal>
        <Drawer.Overlay class="sheet-overlay" onClick={dismiss} />
        <Drawer.Content
          ref={(el) => (content = el)}
          class={["sheet", { tall: props.tall }]}
          aria-label={props.title}
          tabindex={-1}
          onOpenAutoFocus={(e: Event) => {
            e.preventDefault();
            target()?.focus();
          }}
        >
          <div class="sheet-grip" aria-hidden="true" />
          <Drawer.Title class="visually-hidden">{props.title}</Drawer.Title>
          {props.children}
        </Drawer.Content>
      </Drawer.Portal>
      <Closed onClosed={() => props.onClose()} />
    </Drawer>
  );
}

/**
 * Tells the owner once a close has finished animating: the drawer has been open, and now it's shut with nothing in
 * motion. (It starts shut, before it rises, which doesn't count.)
 */
function Closed(props: { onClosed: () => void }) {
  const drawer = Drawer.useContext();
  const shut = createMemo(() => drawer.activeSnapPoint() === 0 && !drawer.isTransitioning() && !drawer.isDragging());
  let opened = false;
  createEffect(shut, (isShut) => {
    if (!isShut) opened = true;
    else if (opened) props.onClosed();
  });
  return null;
}
