import { useLocation, useMatch, useNavigate, useSearchParams } from "@solidjs/router";
import { createMemo, createSignal, For, Show, useContext } from "solid-js";
import type { Element } from "solid-js";
import { ago } from "./format.ts";
import { Icon } from "./Icon.tsx";
import { useOverlay } from "./nav.ts";
import { paths, queryString, readFind, sortParam, tagKey, viewParam, withFind } from "./routes.ts";
import { AppContext, deckTree, inboxCount } from "./state.ts";

const EDGE_OPEN = 1 / 3;
/** How far in from the left a drag pulls the sidebar even over the review's card, which otherwise swipes to answer. */
export const DRAWER_EDGE = 28;
/** Wide enough that the sidebar stays open beside the page, so there's nothing to drag. */
export const wide = () => window.matchMedia("(min-width: 900px)").matches;

/**
 * The deck sidebar, pulled over from the left: a horizontal drag anywhere on the page opens it (and a drag back, or a
 * tap on the page, closes it). On wide screens it stays open beside the page, as on the desktop.
 */
export function Drawer(props: { children: Element }) {
  const app = useContext(AppContext);
  const [dragging, setDragging] = createSignal(false);
  const width = () => Math.min(290, Math.round(window.innerWidth * 0.74));
  const open = () => app.state.drawer;
  const setOpen = (value: boolean) => app.setState((s) => void (s.drawer = value));

  // A drag moves the drawer, page and scrim directly, once a frame, rather than through the shell's style: anything
  // set there is inherited by the whole page, which would restyle every card on every frame.
  let drawer!: HTMLElement;
  let page!: HTMLDivElement;
  let scrim!: HTMLDivElement;
  let at = 0;
  let frame = 0;
  const paint = () => {
    frame = 0;
    const w = width();
    drawer.style.transform = `translate3d(${(at - w) * 0.3}px, 0, 0)`;
    page.style.transform = `translate3d(${at}px, 0, 0)`;
    scrim.style.opacity = String(at / w);
  };
  const release = () => {
    cancelAnimationFrame(frame);
    frame = 0;
    for (const el of [drawer, page, scrim]) el.style.removeProperty(el === scrim ? "opacity" : "transform");
  };

  let gesture: { x: number; y: number; t: number; from: number; horizontal: boolean | null } | null = null;

  const gestures = (el: HTMLElement) => {
    el.addEventListener(
      "touchstart",
      (e) => {
        if (e.touches.length !== 1 || wide()) return;
        const t = e.touches[0]!;
        // Leave sideways scrollers (code blocks, tag rows) alone, and the review, which swipes to answer, but for its
        // left edge and its answer buttons.
        const target = e.target as HTMLElement;
        if (target.closest("[data-scroll-x], textarea, input")) return;
        if (target.closest(".review") && !target.closest(".answers") && t.clientX >= DRAWER_EDGE) return;
        gesture = { x: t.clientX, y: t.clientY, t: e.timeStamp, from: open() ? width() : 0, horizontal: null };
      },
      { passive: true },
    );
    el.addEventListener(
      "touchmove",
      (e) => {
        if (!gesture) return;
        const t = e.touches[0]!;
        const dx = t.clientX - gesture.x;
        const dy = t.clientY - gesture.y;
        if (gesture.horizontal === null) {
          if (Math.abs(dx) < 8 && Math.abs(dy) < 8) return;
          gesture.horizontal = Math.abs(dx) > Math.abs(dy) * 1.4 && (open() ? dx < 0 : dx > 0);
          if (!gesture.horizontal) {
            gesture = null;
            return;
          }
          // Start from where the finger is, not where it first touched, so the drawer doesn't jump.
          gesture.x = t.clientX;
          setDragging(true);
        }
        e.preventDefault();
        at = Math.max(0, Math.min(width(), gesture.from + t.clientX - gesture.x));
        if (!frame) frame = requestAnimationFrame(paint);
      },
      { passive: false },
    );
    const end = (e: TouchEvent) => {
      const g = gesture;
      gesture = null;
      if (!g || !dragging()) return;
      const velocity = (at - g.from) / Math.max(1, e.timeStamp - g.t);
      setOpen(velocity > 0.4 || (velocity > -0.4 && at > width() * (g.from ? 1 - EDGE_OPEN : EDGE_OPEN)));
      setDragging(false);
      // Back to the classes' positions, which the transition animates to from where the drag left off.
      release();
    };
    el.addEventListener("touchend", end);
    el.addEventListener("touchcancel", end);
  };

  return (
    <div
      class={["shell", { dragging: dragging(), open: open() }]}
      ref={gestures}
      style={{ "--drawer-width": `${width()}px` }}
    >
      <aside class="drawer" ref={drawer} aria-label="Decks" aria-hidden={!open() ? "true" : undefined}>
        <Sidebar />
      </aside>
      <div class="page" ref={page}>
        {props.children}
        <div class="scrim" ref={scrim} onClick={() => setOpen(false)} />
      </div>
    </div>
  );
}

function Sidebar() {
  const app = useContext(AppContext);
  const navigate = useNavigate();
  const location = useLocation();
  const overlay = useOverlay();
  const [search] = useSearchParams();
  const finding = () => readFind(search);
  const open = useMatch(() => "/decks/:deck/*rest");
  const tagOpen = useMatch(() => "/tags/:tag");
  const tree = createMemo(() => deckTree(app.state.decks));
  const tags = createMemo(() => [...app.state.tags].sort((a, b) => a.tag.localeCompare(b.tag)));
  const [naming, setNaming] = createSignal(false);
  const close = () => app.setState((s) => void (s.drawer = false));
  /** A deck or tag's link opens it with the view and sort it was last left in. */
  const lasting = (page: string) => {
    const p = app.prefsOf(page);
    return queryString({ view: viewParam(p.view), sort: sortParam(p.sort) });
  };
  const deckLink = (id: string) => paths.deck(id, lasting(id));
  const [name, setName] = createSignal("");

  const create = async () => {
    const value = name().trim();
    setNaming(false);
    setName("");
    if (!value) return;
    const deck = await app.write((c) => c.createDeck(value, null));
    if (deck) navigate(paths.deck(deck.id));
  };

  return (
    <div class="sidebar">
      <a
        href={paths.inbox()}
        class={["deck-row", { selected: location.pathname === paths.inbox() }]}
        onClick={close}
      >
        <Icon name="inbox" />
        <span class="deck-name">Review</span>
        <span class="deck-counts">
          <Show when={inboxCount(app.state.decks)}>{(n) => <span class="due">{n()}</span>}</Show>
        </span>
      </a>
      <a
        href={withFind(location.pathname + location.search, finding() ?? "")}
        class={["deck-row", "search-row", { selected: finding() !== null }]}
        onClick={(e) => {
          e.preventDefault();
          close();
          // Over whatever's open, so closing it comes back here.
          if (finding() === null) overlay.open(withFind(location.pathname + location.search, ""));
        }}
      >
        <Icon name="search" />
        <span class="deck-name">Search</span>
      </a>
      <header class="sidebar-head">
        <span class="section-title">Decks</span>
        <button class="icon-button" aria-label="New deck" onClick={() => setNaming(true)}>
          <Icon name="plus" />
        </button>
      </header>
      <nav class="deck-tree">
        <For each={tree()} keyed={(row) => row.deck.id}>
          {(row) => (
            <a
              href={deckLink(row().deck.id)}
              class={["deck-row", { selected: row().deck.id === open()?.params.deck }]}
              style={{ "--depth": String(row().depth) }}
              onClick={close}
            >
              <Icon name={row().depth ? "subdeck" : "deck"} />
              <span class="deck-name">{row().deck.name}</span>
              <span class="deck-counts">
                <Show when={row().deck.due}>
                  <span class="due">{row().deck.due}</span>
                </Show>
                <Show when={row().deck.new}>
                  <span class="new">{row().deck.new}</span>
                </Show>
              </span>
            </a>
          )}
        </For>
        <Show when={naming()}>
          <form
            class="deck-row naming"
            onSubmit={(e) => {
              e.preventDefault();
              void create();
            }}
          >
            <Icon name="deck" />
            <input
              ref={(el) => queueMicrotask(() => el.focus())}
              placeholder="New deck"
              enterkeyhint="done"
              value={name()}
              onInput={(e) => setName(e.currentTarget.value)}
              onBlur={() => void create()}
              onKeyDown={(e) => {
                if (e.key === "Escape") {
                  setName("");
                  setNaming(false);
                }
              }}
            />
          </form>
        </Show>
        <Show when={tags().length}>
          <header class="sidebar-head tags-head">
            <span class="section-title">Tags</span>
          </header>
          <For each={tags()} keyed={(t) => t.tag}>
            {(t) => (
              <a
                href={paths.tag(t().tag, lasting(tagKey(t().tag)))}
                class={["deck-row", { selected: t().tag === tagOpen()?.params.tag }]}
                onClick={close}
              >
                <Icon name="tag" />
                <span class="deck-name">{t().tag}</span>
                <span class="deck-counts">
                  <span class="new">{t().cards}</span>
                </span>
              </a>
            )}
          </For>
        </Show>
      </nav>
      <a
        href={paths.settings()}
        class={["sync-row", { selected: location.pathname === paths.settings() }]}
        onClick={close}
      >
        <Icon name="cloud" />
        <span>Sync</span>
        <span class="faint">{app.state.lastSync ? ago(app.state.lastSync) : "Not synced yet"}</span>
      </a>
    </div>
  );
}
