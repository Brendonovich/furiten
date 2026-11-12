import { useLocation, useSearchParams } from "@solidjs/router";
import { createEffect, createMemo, createSignal, flush, For, onSettled, Show, untrack, useContext } from "solid-js";
import { CardFace } from "./Card.tsx";
import { DRAWER_EDGE, wide } from "./Drawer.tsx";
import { counts, plural, until } from "./format.ts";
import { Icon } from "./Icon.tsx";
import { useOverlay } from "./nav.ts";
import { deckCards, deckQuery, paths, readFind, readText, searchCards, tagCards, withFind } from "./routes.ts";
import { AppContext, byDeck, deckPath } from "./state.ts";
import type { CardView, NextReview } from "./types.ts";

const label = (remembered: boolean) => (remembered ? "Remembered" : "Forgot");
/** How far a card is dragged before letting go answers it. */
const SWIPE = 96;
/** How long an answered card takes to fly off (`.review-card.leaving` in styles.css). */
const FLY_OFF = 220;

/**
 * A cram (docs/contract.md, "Scheduling"): the cards a list is showing, in its order, due or not. Answers record
 * nothing; a forgotten card goes to the back until it's remembered. `back` is the list's URL, for closing.
 */
export type Cram = { title: string; cards: () => Promise<CardView[]>; back: string };

/** The cards a list was showing when its Review was pressed, for the cram it opened at `at`. */
let pressed: { at: string; cards: CardView[] } | null = null;

/**
 * Review, centred at the foot of a list: a cram of exactly the cards it's showing (`null` while they load), opened at
 * `at`. A reload there lists them again from the URL.
 */
export function CramButton(props: { cards: readonly CardView[] | null; at: string }) {
  const overlay = useOverlay();
  return (
    <Show when={props.cards?.length}>
      <button
        data-component="button-v2" data-variant="contrast" data-size="touch" class="cram"
        title="Review these cards, without changing when they’re due"
        onClick={() => {
          pressed = { at: props.at, cards: [...(props.cards ?? [])] };
          overlay.open(props.at);
        }}
      >
        <Icon name="play" />
        Review
      </button>
    </Show>
  );
}

/** A cram's cards: the ones its list showed when Review was pressed, or after a reload, listed again with `list`. */
function cramCards(at: string, list: () => Promise<CardView[]>): Promise<CardView[]> {
  const p = pressed;
  pressed = null;
  return p?.at === at ? Promise.resolve(p.cards) : list();
}

/** `/decks/:deck/cram`: the deck's page as its URL has it, searched, filtered and sorted. */
export function DeckCram(props: { deck: string }) {
  const app = useContext(AppContext);
  const [search] = useSearchParams();
  const { deck, query } = untrack(() => ({ deck: props.deck, query: deckQuery(search) }));
  const name = () => app.state.decks.find((d) => d.id === deck)?.name ?? "Review";
  const list = () => app.client()?.cards(deckCards(deck, search)) ?? Promise.resolve([]);
  return (
    <Review
      deck={deck}
      cram={{
        get title() {
          return name();
        },
        cards: () => cramCards(paths.deckCram(deck, query), list),
        back: paths.deck(deck, query),
      }}
    />
  );
}

/** `/tags/:tag/cram`: every card with the tag, as its page lists them, searched and sorted. */
export function TagCram(props: { tag: string }) {
  const app = useContext(AppContext);
  const [search] = useSearchParams();
  const { tag, query } = untrack(() => ({ tag: props.tag, query: deckQuery(search) }));
  const list = () => app.client()?.cards(tagCards(tag, search)) ?? Promise.resolve([]);
  return (
    <Review
      deck={null}
      cram={{ title: `#${tag}`, cards: () => cramCards(paths.tagCram(tag, query), list), back: paths.tag(tag, query) }}
    />
  );
}

/** `?find=…&cram=find`: what the search layer found, deck by deck as it shows them, over it. */
export function SearchCram() {
  const app = useContext(AppContext);
  const location = useLocation();
  const [search] = useSearchParams();
  const { url, find } = untrack(() => ({ url: location.pathname + location.search, find: readFind(search) ?? "" }));
  const q = find.trim();
  const list = async () => {
    const cards = q ? ((await app.client()?.cards(searchCards(q))) ?? []) : [];
    return byDeck(app.state.decks, cards).flatMap((g) => g.cards);
  };
  return (
    <Review
      deck={null}
      cram={{ title: `“${q}”`, cards: () => cramCards(withFind(url, find, true), list), back: withFind(url, find) }}
    />
  );
}

/**
 * One review session over a deck (and its subdecks), or with no deck, the inbox: every due card from every deck,
 * shown in place rather than over the deck. Sides reveal one at a time with a tap, as on the desktop. Swipe right if
 * you remembered it or left if you forgot, or use the buttons, at any side. With `cram`, a cram over the list instead.
 */
export function Review(props: { deck: string | null; cram?: Cram }) {
  const app = useContext(AppContext);
  const overlay = useOverlay();
  const [search] = useSearchParams();
  const [next, setNext] = createSignal<NextReview | null>(null);
  const [shown, setShown] = createSignal(1);
  const [answered, setAnswered] = createSignal(0);
  const [drag, setDrag] = createSignal(0);
  const [leaving, setLeaving] = createSignal<0 | 1 | -1>(0);
  const [busy, setBusy] = createSignal(false);
  let started = Date.now();
  let queue: CardView[] | null = null;
  const [cramTotal, setCramTotal] = createSignal(0);
  const [forgotten, setForgotten] = createSignal(0);

  const inbox = () => props.deck === null && !props.cram;
  const deck = createMemo(() => app.state.decks.find((d) => d.id === props.deck));
  const card = () => next()?.card ?? null;
  const revealed = () => {
    const c = card();
    return c !== null && shown() >= c.sides.length;
  };

  const load = async () => {
    const c = app.client();
    if (!c) return;
    try {
      let n: NextReview;
      if (props.cram) {
        if (!queue) {
          queue = await props.cram.cards();
          setCramTotal(queue.length);
        }
        n = { card: queue[0] ?? null, remaining: { due: queue.length, new: 0 } };
      } else {
        n = await c.next(props.deck ?? undefined);
      }
      setNext(n);
      setShown(1);
      setDrag(0);
      setLeaving(0);
      started = Date.now();
    } catch (e) {
      app.fail(e);
    }
  };

  // The inbox stays open, so cards added or come due elsewhere show up once it's run out.
  createEffect(
    () => app.state.lastSync,
    () => void (inbox() && next() && !card() && !busy() && load()),
    { defer: true },
  );

  const close = () => {
    if (props.cram) return overlay.close(props.cram.back);
    const d = props.deck;
    if (d === null) return;
    overlay.close(paths.deck(d, deckQuery(search)));
    void app.refresh();
  };

  let cardEl: HTMLElement | undefined;
  // The card grows to fit the new side rather than jumping to its new height.
  const reveal = () => {
    const c = card();
    if (!c || shown() >= c.sides.length) return;
    const from = cardEl?.offsetHeight;
    setShown(shown() + 1);
    flush();
    const to = cardEl?.offsetHeight;
    if (cardEl && from !== undefined && to !== from) {
      cardEl.animate({ height: [`${from}px`, `${to}px`], overflow: ["hidden", "hidden"] }, { duration: 280, easing: "cubic-bezier(0.2, 0.8, 0.2, 1)" });
    }
  };

  const answer = async (remembered: boolean) => {
    const c = card();
    const client = app.client();
    if (!c || !client || busy()) return;
    setBusy(true);
    // The card flies off the way a swipe would send it, and the next one comes in once it's gone.
    setLeaving(remembered ? 1 : -1);
    try {
      await Promise.all([
        // A cram records nothing: a forgotten card only goes to the back of it.
        props.cram ? cramAnswer(remembered) : client.answer(c.id, remembered, Date.now() - started),
        new Promise((done) => setTimeout(done, FLY_OFF)),
      ]);
      setAnswered(answered() + 1);
      await load();
      // Keep the sidebar's counts current.
      if (inbox()) void app.refreshDecks();
    } catch (e) {
      app.fail(e);
      setLeaving(0);
      setDrag(0);
    } finally {
      setBusy(false);
    }
  };

  const cramAnswer = async (remembered: boolean) => {
    const card = queue?.shift();
    if (card && !remembered) {
      queue!.push(card);
      setForgotten(forgotten() + 1);
    }
  };

  onSettled(() => {
    void load();
    const onKey = (e: KeyboardEvent) => {
      if (readText(search, "edit") || (e.target as HTMLElement).closest("input, textarea")) return;
      if (e.key === "Escape") return close();
      if (e.key === " " || e.key === "Enter") {
        e.preventDefault();
        return reveal();
      }
      if (e.key === "1") void answer(false);
      if (e.key === "2") void answer(true);
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  });

  // Swiping anywhere below the toolbar: the card follows the finger, at any side; past SWIPE it flies off and answers.
  // In the inbox, a drag from the left edge or on the answer buttons pulls the sidebar instead (Drawer.tsx).
  let touch: { x: number; y: number; horizontal: boolean | null; moved: boolean } | null = null;
  const swipe = (el: HTMLElement) => {
    el.addEventListener(
      "touchstart",
      (e) => {
        const t = e.touches[0]!;
        touch = null;
        if (!card() || (e.target as HTMLElement).closest("header, .answers")) return;
        if (inbox() && !wide() && t.clientX < DRAWER_EDGE) return;
        touch = { x: t.clientX, y: t.clientY, horizontal: null, moved: false };
        suppressTap = false;
      },
      { passive: true },
    );
    el.addEventListener(
      "touchmove",
      (e) => {
        if (!touch) return;
        const t = e.touches[0]!;
        const dx = t.clientX - touch.x;
        const dy = t.clientY - touch.y;
        if (touch.horizontal === null) {
          if (Math.abs(dx) < 8 && Math.abs(dy) < 8) return;
          touch.horizontal = Math.abs(dx) > Math.abs(dy);
        }
        touch.moved = true;
        if (!touch.horizontal || busy()) return;
        e.preventDefault();
        setDrag(dx);
      },
      { passive: false },
    );
    el.addEventListener("touchend", () => {
      if (!touch) return;
      const moved = touch.moved;
      touch = null;
      const dx = drag();
      if (Math.abs(dx) >= SWIPE) {
        void answer(dx > 0);
      } else {
        setDrag(0);
      }
      if (moved) suppressTap = true;
    });
  };
  let suppressTap = false;

  const tilt = () => {
    const l = leaving();
    // Just far enough to be off the screen, so it's gone as the fly-off ends.
    const dx = l ? l * window.innerWidth : drag();
    return `translateX(${dx}px) rotate(${dx / 24}deg)`;
  };
  const intent = () => {
    const dx = leaving() ? leaving() * SWIPE : drag();
    return Math.max(-1, Math.min(1, dx / SWIPE));
  };
  /** How far toward answering this way the swipe is, from 0 to 1. */
  const toward = (remembered: boolean) => Math.max(0, remembered ? intent() : -intent());
  // The card's edge takes the answer's colour as it's dragged: it starts a little way in and is full half again past
  // the point letting go answers, so it keeps building as you commit.
  const edge = () => {
    if (leaving()) return 1;
    return Math.max(0, Math.min(1, (Math.abs(drag()) - SWIPE / 4) / (SWIPE * 1.25)));
  };
  const swipeColour = () =>
    `color-mix(in srgb, var(--${label(intent() > 0).toLowerCase()}) ${edge() * 100}%, transparent)`;

  return (
    <div
      class={["review", { inline: inbox() }]}
      role={inbox() ? undefined : "dialog"}
      aria-label="Review"
      ref={swipe}
      onClick={(e) => {
        if (suppressTap) {
          suppressTap = false;
          return;
        }
        // A tap anywhere but the toolbar and buttons shows the next side.
        if (!(e.target as HTMLElement).closest("header, button, a")) reveal();
      }}
    >
      <header class="toolbar">
        <Show
          when={!inbox()}
          fallback={
            <button class="icon-button" aria-label="Decks" onClick={() => app.setState((s) => void (s.drawer = true))}>
              <Icon name="menu" />
            </button>
          }
        >
          <button class="icon-button" aria-label="Done" onClick={close}>
            <Icon name="close" />
          </button>
        </Show>
        <div class="toolbar-title stacked">
          <span>{props.cram?.title ?? deck()?.name ?? "Review"}</span>
          <Show when={next()}>
            {(n) => (
              <span class="faint small">
                {props.cram ? `${n().remaining.due} left` : counts(n().remaining.due, n().remaining.new)}
              </span>
            )}
          </Show>
        </div>
        <Show when={!inbox()} fallback={<span class="icon-button toolbar-balance" aria-hidden="true" />}>
          <button data-component="button-v2" data-variant="ghost" data-size="touch" onClick={close}>
            Done
          </button>
        </Show>
      </header>

      <Show
        when={next()}
        fallback={<p class="center faint">Loading…</p>}
      >
        <Show
          when={card()}
          fallback={
            <Show when={!inbox()} fallback={<CaughtUp answered={answered()} />}>
              <Show
                when={props.cram}
                fallback={<Finished answered={answered()} onClose={close} />}
              >
                <Crammed cards={cramTotal()} forgotten={forgotten()} onClose={close} />
              </Show>
            </Show>
          }
        >
          {(c) => (
            <>
              <div class="review-stage">
                {/* Keyed, so each card animates in afresh rather than only its text changing. */}
                <Show when={c()} keyed>
                  {(current) => (
                    <div class="review-column">
                      <Show when={inbox() || current.deck_id !== props.deck}>
                        <p class="review-deck faint small">
                          <Icon name="deck" />
                          {deckPath(app.state.decks, current.deck_id)}
                        </p>
                      </Show>
                      <div
                        class={["review-card", { settling: drag() === 0, leaving: leaving() !== 0 }]}
                        ref={(el) => (cardEl = el)}
                        style={{ transform: tilt(), "--swipe": swipeColour() }}
                      >
                        <For each={current.sides.slice(0, shown())}>
                          {(side, i) => (
                            <>
                              <Show when={i() > 0}>
                                <hr class="side-rule" />
                              </Show>
                              <div class={{ "reveal-in": i() > 0 }}>
                                <CardFace text={side} />
                              </div>
                            </>
                          )}
                        </For>
                        <Show when={revealed() && current.tags.length > 0}>
                          <div class={["tags review-tags", { "reveal-in": current.sides.length > 1 }]}>
                            <For each={current.tags}>{(t) => <span class="badge">#{t}</span>}</For>
                          </div>
                        </Show>
                      </div>
                      <Show when={!revealed()}>
                        {/* Fades as the card is swiped away. */}
                        <p
                          class={["tap-hint faint", { settling: drag() === 0 || leaving() !== 0 }]}
                          style={{ opacity: String(1 - Math.abs(intent())) }}
                        >
                          Tap to reveal the next side
                        </p>
                      </Show>
                    </div>
                  )}
                </Show>
              </div>
              <footer class="review-controls">
                <div class="answers">
                  <For each={[false, true]}>
                    {(remembered) => (
                      <button
                        data-component="button-v2"
                        data-variant="neutral"
                        data-size="touch"
                        class={["answer", `answer-${label(remembered).toLowerCase()}`]}
                        // A swipe presses its answer's button and fades the other.
                        style={{ "--press": String(toward(remembered)), "--dim": String(toward(!remembered)) }}
                        aria-label={label(remembered)}
                        title={`${label(remembered)} (${remembered ? 2 : 1})`}
                        disabled={busy()}
                        onClick={() => void answer(remembered)}
                      >
                        <Icon name={remembered ? "check" : "close"} />
                      </button>
                    )}
                  </For>
                </div>
              </footer>
            </>
          )}
        </Show>
      </Show>
    </div>
  );
}

/** The inbox with nothing left: every deck is done for now. */
function CaughtUp(props: { answered: number }) {
  return (
    <div class="empty">
      <h2>{props.answered > 0 ? "That’s everything for now" : "All caught up"}</h2>
      <p class="muted">Cards from every deck show up here when they’re due.</p>
    </div>
  );
}

/** The end of a cram: every card remembered once. */
function Crammed(props: { cards: number; forgotten: number; onClose: () => void }) {
  return (
    <div class="empty">
      <h2>{props.cards > 0 ? "That’s every card" : "No cards to review"}</h2>
      <Show when={props.cards > 0}>
        <p class="muted">
          {plural(props.cards, "card")}
          {props.forgotten > 0 ? `, ${props.forgotten} forgotten along the way` : ", none forgotten"}. Cramming leaves
          their schedules as they were.
        </p>
      </Show>
      <div class="sheet-buttons">
        <button data-component="button-v2" data-variant="contrast" data-size="touch" onClick={() => props.onClose()}>
          Done
        </button>
      </div>
    </div>
  );
}

function Finished(props: { answered: number; onClose: () => void }) {
  const app = useContext(AppContext);
  const overlay = useOverlay();
  const nextDue = createMemo(() => {
    const due = app.state.cards.filter((c) => c.schedule.state !== "new").map((c) => c.schedule.due);
    return due.length ? Math.min(...due) : null;
  });
  return (
    <div class="empty">
      <h2>{props.answered > 0 ? "That’s everything for now" : "Nothing to review"}</h2>
      <p class="muted">
        <Show when={nextDue()} fallback="Add cards and they’ll show up here.">
          {(d) => <>The next card is due {until(d())}.</>}
        </Show>
      </p>
      <div class="sheet-buttons">
        <button
          data-component="button-v2" data-variant="neutral" data-size="touch"
          onClick={() => overlay.open(overlay.with({ edit: "new" }))}
        >
          Add cards
        </button>
        <button data-component="button-v2" data-variant="contrast" data-size="touch" onClick={() => props.onClose()}>
          Done
        </button>
      </div>
    </div>
  );
}
