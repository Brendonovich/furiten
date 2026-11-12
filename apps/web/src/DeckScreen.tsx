import { useNavigate, useParams, useSearchParams } from "@solidjs/router";
import { createEffect, createMemo, createSignal, For, onCleanup, onSettled, Show, untrack, useContext } from "solid-js";
import type { CardQuery } from "./api.ts";
import { CardFace, DueLabel, Tile } from "./Card.tsx";
import { DeckActions } from "./DeckActions.tsx";
import { lapses, plain, reviews, until } from "./format.ts";
import { Field } from "./Field.tsx";
import { Icon, type IconName } from "./Icon.tsx";
import { useOverlay } from "./nav.ts";
import { CramButton } from "./Review.tsx";
import {
  deckCards,
  deckQuery,
  newSeed,
  nextSort,
  paths,
  readSort,
  readStep,
  readText,
  readView,
  type Search,
  sortParams,
  tagCards,
  tagKey,
  viewParam,
} from "./routes.ts";
import { AppContext } from "./state.ts";
import type { CardView, Sort, View } from "./types.ts";

const SORTS: { by: Sort; label: string; short: string; dirs?: [string, string] }[] = [
  { by: "created", label: "Date created", short: "Created", dirs: ["Oldest first", "Newest first"] },
  { by: "updated", label: "Last updated", short: "Updated", dirs: ["Oldest first", "Newest first"] },
  { by: "name", label: "Alphanumeric", short: "Name", dirs: ["A to Z", "Z to A"] },
  { by: "due", label: "Due date", short: "Due", dirs: ["Soonest first", "Latest first"] },
  { by: "reviewed", label: "Last reviewed", short: "Reviewed", dirs: ["Oldest first", "Newest first"] },
  { by: "interval", label: "Last interval", short: "Interval", dirs: ["Shortest first", "Longest first"] },
  { by: "retention", label: "Retention rate", short: "Retention", dirs: ["Lowest first", "Highest first"] },
  { by: "random", label: "Random", short: "Random" },
];

// Sort control variants to try, picked with ?sortbar=icons|sort|short|split (remembered). Temporary.
type SortBar = "icons" | "sort" | "short" | "split";
const SORT_BAR: SortBar = (() => {
  const all: SortBar[] = ["icons", "sort", "short", "split"];
  const asked = new URLSearchParams(location.search).get("sortbar") as SortBar | null;
  if (asked && all.includes(asked)) localStorage.setItem("furiten.sortbar", asked);
  const kept = localStorage.getItem("furiten.sortbar") as SortBar | null;
  return kept && all.includes(kept) ? kept : "short";
})();

const VIEWS: { view: View; label: string; icon: IconName }[] = [
  { view: "grid", label: "Grid view", icon: "grid" },
  { view: "list", label: "List view", icon: "list" },
  { view: "column", label: "Column view", icon: "column" },
];

/**
 * What a page of cards lists: a deck's cards, or every card with a tag, from every deck. Both have the same view, sort
 * and search; a deck's page adds the tag filter and New card.
 */
export type Page = { deck: string; name: string } | { tag: string };

/** What keys the page's cards and its remembered view and sort. */
const pageKey = (page: Page) => ("deck" in page ? page.deck : tagKey(page.tag));
/** The cards the page lists for its URL, which a cram of it lists too. */
const pageCards = (page: Page, search: Search): CardQuery =>
  "deck" in page ? deckCards(page.deck, search) : tagCards(page.tag, search);
const pageCram = (page: Page, search: Search) =>
  "deck" in page ? paths.deckCram(page.deck, deckQuery(search)) : paths.tagCram(page.tag, deckQuery(search));

/** Lists the page's cards for its URL, while it's open. */
export function usePageCards(page: () => Page) {
  const app = useContext(AppContext);
  const [search, setSearch] = useSearchParams();
  const sort = () => readSort(search);

  createEffect(
    () => JSON.stringify(pageCards(page(), search)),
    (key) => void app.showCards(JSON.parse(key) as CardQuery),
  );
  onCleanup(() => void app.showCards(null));

  // A random sort without a seed gets one, so its order holds across refreshes and reloads.
  createEffect(
    () => sort().by === "random" && sort().seed === null,
    (unseeded) => void (unseeded && setSearch({ seed: newSeed() }, { replace: true })),
  );
}

/** A deck's page, `/decks/:deck`: its search, view and sort, the open card and its menu all come from the URL. */
export function DeckScreen() {
  const app = useContext(AppContext);
  const params = useParams<{ deck: string }>();
  const [search] = useSearchParams();
  const navigate = useNavigate();
  const overlay = useOverlay();
  const id = () => params.deck ?? "";
  const deck = createMemo(() => app.state.decks.find((d) => d.id === id()));
  usePageCards(() => ({ deck: id(), name: deck()?.name ?? "" }));

  // A deck that's gone, deleted here or on another device, leaves for Review.
  createEffect(
    () => app.state.loaded && app.state.decks.length > 0 && !deck(),
    (gone) => void (gone && navigate(paths.inbox(), { replace: true })),
  );

  const newCard = () => overlay.open(overlay.with({ edit: "new" }));

  return (
    <div class="screen">
      <header class="toolbar">
        <button class="icon-button" aria-label="Decks" onClick={() => app.setState((s) => void (s.drawer = true))}>
          <Icon name="menu" />
        </button>
        <h1 class="toolbar-title">{deck()?.name ?? "Furiten"}</h1>
        <Show when={deck()}>
          <button class="icon-button" aria-label="Deck actions" onClick={() => overlay.open(overlay.with({ actions: "menu" }))}>
            <Icon name="more" />
          </button>
        </Show>
      </header>

      <Show
        when={deck()}
        fallback={
          <Show when={app.state.loaded} fallback={<p class="center faint">Opening your decks…</p>}>
            <NoDecks />
          </Show>
        }
      >
        {(d) => (
          <>
            <Tools page={{ deck: d().id, name: d().name }} />
            <Cards page={{ deck: d().id, name: d().name }} onNewCard={newCard} />
            <PageFooter page={{ deck: d().id, name: d().name }} onNewCard={newCard} />
            <Show when={readStep(search)}>
              {(step) => (
                <DeckActions
                  deck={d()}
                  step={step()}
                  onStep={(next) => overlay.replace(overlay.with({ actions: next }))}
                  onClose={() => overlay.close(overlay.with({ actions: null }))}
                />
              )}
            </Show>
          </>
        )}
      </Show>
    </div>
  );
}

/** Whatever the page is showing, and setters that put a change in the URL (and remember view and sort). */
function usePageSearch(page: () => Page) {
  const app = useContext(AppContext);
  const [search, setSearch] = useSearchParams();
  const view = () => readView(search);
  const sort = () => readSort(search);
  return {
    search,
    view,
    sort,
    q: () => readText(search, "q"),
    setQ: (q: string) => setSearch({ q: q || undefined }, { replace: true }),
    tag: () => readText(search, "tag"),
    setTag: (tag: string) => setSearch({ tag: tag || undefined }),
    setView: (v: View) => {
      app.remember(pageKey(page()), { view: v, sort: sort() });
      setSearch({ view: viewParam(v) });
    },
    /** Picking the current sort keeps its direction; picking another starts in its default direction. */
    setSort: (by: Sort, desc?: boolean) => {
      const next = nextSort(sort(), by, desc);
      app.remember(pageKey(page()), { view: view(), sort: next });
      setSearch(sortParams(next));
    },
  };
}

export function Tools(props: { page: Page }) {
  const url = usePageSearch(() => props.page);
  return (
    <div class="tools">
      <div class="tool-row">
        <div class="segmented" role="radiogroup" aria-label="View">
          <For each={VIEWS}>
            {(v) => (
              <button
                role="radio"
                aria-checked={url.view() === v.view ? "true" : "false"}
                aria-label={v.label}
                class={{ active: url.view() === v.view }}
                onClick={() => url.setView(v.view)}
              >
                <Icon name={v.icon} />
              </button>
            )}
          </For>
        </div>
        <Show when={"deck" in props.page && props.page.deck}>{(deck) => <TagFilter page={props.page} deck={deck()} />}</Show>
        <div class="sort-controls">
          <SortControls page={props.page} />
        </div>
      </div>
    </div>
  );
}

/**
 * Search, Review and New card (on a deck's page), floating over the cards at the foot of the page, as on iOS: Review, in
 * the middle, crams the cards the page lists (Review.tsx); search is a button on the left that
 * opens into a field across the footer (New card steps aside for a button that closes it), and stays open while there's
 * a search. The field leads the URL while you type; Back, Clear search or another page set it from the URL.
 */
export function PageFooter(props: { page: Page; onNewCard?: () => void }) {
  const app = useContext(AppContext);
  const url = usePageSearch(() => props.page);
  let timer: ReturnType<typeof setTimeout> | undefined;
  let input: HTMLInputElement | undefined;
  const [draft, setDraft] = createSignal(untrack(url.q));
  const [focused, setFocused] = createSignal(false);
  const open = () => focused() || !!draft();
  let written: string | null = null;
  createEffect(url.q, (q) => {
    if (q === written) return;
    written = null;
    setDraft(q);
  });
  const commit = (q: string) => {
    clearTimeout(timer);
    written = q;
    url.setQ(q);
  };
  const close = () => {
    setDraft("");
    commit("");
    input?.blur();
    setFocused(false);
  };

  let footer!: HTMLElement;
  onSettled(() => liftOverKeyboard(footer));

  return (
    <footer ref={footer} class={["deck-footer", { searching: open() }]}>
      <Show
        when={open()}
        fallback={
          <button
            data-component="button-v2" data-variant="neutral" data-size="touch" class="fab" aria-label="Search cards"
            onClick={() => {
              setFocused(true);
              queueMicrotask(() => input?.focus());
            }}
          >
            <Icon name="search" />
          </button>
        }
      >
        <Field
          ref={(el) => (input = el)}
          class="search"
          type="search"
          placeholder="Search cards"
          aria-label="Search cards"
          enterkeyhint="search"
          leading={<Icon name="search" />}
          value={draft()}
          onFocus={() => setFocused(true)}
          onBlur={() => setFocused(false)}
          onInput={(e) => {
            const q = e.currentTarget.value;
            setDraft(q);
            clearTimeout(timer);
            timer = setTimeout(() => commit(q), 150);
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              commit(e.currentTarget.value);
              e.currentTarget.blur();
            } else if (e.key === "Escape") {
              close();
            }
          }}
        />
      </Show>
      <span class="spacer" />
      {/* Out of the way while typing; back between the field and Close once the search is in. */}
      <Show when={!focused()}>
        <CramButton
          cards={app.cardsAre(pageCards(props.page, url.search)) ? app.state.cards : null}
          at={pageCram(props.page, url.search)}
        />
      </Show>
      <span class="spacer" />
      <Show
        when={open()}
        fallback={
          <Show when={props.onNewCard} fallback={<span class="fab-balance" aria-hidden="true" />}>
            {(onNewCard) => (
              <button data-component="button-v2" data-variant="neutral" data-size="touch" class="fab" aria-label="New card" onClick={() => onNewCard()()}>
                <Icon name="plus" />
              </button>
            )}
          </Show>
        }
      >
        <button
          data-component="button-v2" data-variant="neutral" data-size="touch" class="fab" aria-label="Close search"
          // Before the field's blur, so a tap here clears the search rather than only closing the keyboard.
          onPointerDown={(e) => e.preventDefault()}
          onClick={close}
        >
          <Icon name="close" />
        </button>
      </Show>
    </footer>
  );
}

/**
 * Lifts a footer pinned to the foot of the page onto the keyboard. iOS doesn't resize the page for it
 * (`interactive-widget` is Chrome's), so this reads how much of the layout the visual viewport has lost at the bottom
 * and lifts the footer by that. Returns the cleanup.
 */
export function liftOverKeyboard(footer: HTMLElement): (() => void) | undefined {
  const vv = window.visualViewport;
  if (!vv) return;
  // Measured from where the footer rests (the bottom of the page it's pinned to, which the lift doesn't move, even
  // mid-transition), not the window's height, which in an installed app needn't be where the page ends.
  const update = () => {
    const bottom = (footer.offsetParent ?? document.documentElement).getBoundingClientRect().bottom;
    const lift = Math.max(0, Math.round(bottom - (vv.height + vv.offsetTop)));
    footer.style.setProperty("--keyboard", `${lift}px`);
    footer.classList.toggle("over-keyboard", lift > 0);
  };
  update();
  vv.addEventListener("resize", update);
  vv.addEventListener("scroll", update);
  return () => {
    vv.removeEventListener("resize", update);
    vv.removeEventListener("scroll", update);
  };
}

/** Shows only the cards with one tag: a native picker of the library's tags, this deck's first. */
function TagFilter(props: { page: Page; deck: string }) {
  const app = useContext(AppContext);
  const url = usePageSearch(() => props.page);
  const [tags, setTags] = createSignal<string[]>([]);
  createEffect(
    () => [props.deck, app.state.lastSync] as const,
    ([deck]) => {
      void app.client()?.tags(deck).then((t) => setTags(t.map((x) => x.tag)), () => setTags([]));
    },
  );
  // A tag from a link stays pickable even if no card carries it any more.
  const options = () => (url.tag() && !tags().includes(url.tag()) ? [url.tag(), ...tags()] : tags());

  return (
    <div class="segmented sortbar">
      <label
        class={["sort-pick", { labelled: !!url.tag(), active: !!url.tag() }]}
        title={url.tag() ? `Only cards tagged #${url.tag()}` : "Filter by tag"}
      >
        <Icon name="filter" />
        <Show when={url.tag()}>{(t) => <span>#{t()}</span>}</Show>
        <select aria-label="Filter by tag" value={url.tag()} onChange={(e) => url.setTag(e.currentTarget.value)}>
          <option value="">All cards</option>
          <For each={options()}>{(t) => <option value={t}>#{t}</option>}</For>
        </select>
      </label>
    </div>
  );
}

/** What the cards are sorted by and which way round, in the variant picked by SORT_BAR. */
function SortControls(props: { page: Page }) {
  const url = usePageSearch(() => props.page);
  const sort = url.sort;
  const current = () => SORTS.find((s) => s.by === sort().by) ?? SORTS[0]!;
  const dirLabel = () => {
    const dirs = current().dirs;
    return dirs ? dirs[sort().desc ? 1 : 0] : "Shuffle again";
  };
  const setDesc = (desc: boolean) => url.setSort(sort().by, desc);
  const flip = () => (current().dirs ? setDesc(!sort().desc) : url.setSort("random"));

  const picker = (inner: () => any, cls: string) => (
    <label class={cls} title={`Sort by ${current().label.toLowerCase()}`}>
      {inner()}
      <select
        aria-label="Sort by"
        value={sort().by}
        onChange={(e) => url.setSort(e.currentTarget.value as Sort)}
      >
        <For each={SORTS}>{(s) => <option value={s.by}>{s.label}</option>}</For>
      </select>
    </label>
  );
  const dirButton = (cls: string) => (
    <button class={cls} aria-label={dirLabel()} title={dirLabel()} onClick={flip}>
      <Icon name={!current().dirs ? "shuffle" : sort().desc ? "down" : "up"} />
    </button>
  );

  switch (SORT_BAR) {
    // [≡][↑]: one icon group, the twin of the view switcher.
    case "icons":
      return (
        <div class="segmented sortbar">
          {picker(() => <Icon name="order" />, "sort-pick")}
          {dirButton("")}
        </div>
      );
    // [⇅ Sort] [↑]: the same word whatever the sort; the menu and tooltip say which.
    case "sort":
      return (
        <>
          {picker(() => (<><Icon name="sort" /><span>Sort</span></>), "select")}
          {dirButton("select icon-only")}
        </>
      );
    // [Retention | ↑]: short names, as wide as the current one; it sits on the right, so the arrow never moves.
    case "short":
      return (
        <div class="segmented sortbar">
          {picker(() => (<><Icon name="order" /><span>{current().short}</span></>), "sort-pick labelled")}
          {dirButton("")}
        </div>
      );
    // [≡] [↑|↓]: both directions on show, the current one lit.
    case "split":
      return (
        <>
          <div class="segmented sortbar">{picker(() => <Icon name="order" />, "sort-pick")}</div>
          <Show
            when={current().dirs}
            fallback={<div class="segmented sortbar">{dirButton("")}</div>}
          >
            {(dirs) => (
              <div class="segmented" role="radiogroup" aria-label="Direction">
                <For each={[false, true]}>
                  {(desc) => (
                    <button
                      role="radio"
                      aria-checked={sort().desc === desc ? "true" : "false"}
                      aria-label={dirs()[desc ? 1 : 0]}
                      title={dirs()[desc ? 1 : 0]}
                      class={{ active: sort().desc === desc }}
                      onClick={() => setDesc(desc)}
                    >
                      <Icon name={desc ? "down" : "up"} />
                    </button>
                  )}
                </For>
              </div>
            )}
          </Show>
        </>
      );
  }
}

/** The page's cards as its view has them, with New card ahead of them when there's `onNewCard`. */
export function Cards(props: { page: Page; onNewCard?: () => void }) {
  const app = useContext(AppContext);
  const url = usePageSearch(() => props.page);
  const overlay = useOverlay();
  const view = url.view;
  const mine = () => app.state.cardsFor === pageKey(props.page);
  const cards = () => (mine() ? app.state.cards : []);
  const edit = (card: CardView) => overlay.open(overlay.with({ edit: card.id }));
  // The card open in column view, `?card=<id>`: picking another replaces it, so Back closes the detail.
  const selected = () => readText(url.search, "card") || null;
  const setSelected = (id: string | null) => {
    const to = overlay.with({ card: id });
    if (id === null) overlay.close(to);
    else if (selected()) overlay.replace(to);
    else overlay.open(to);
  };
  const detail = createMemo(() => cards().find((c) => c.id === selected()));

  return (
    <div class="cards-scroll">
      <Show when={mine()} fallback={<p class="center faint">Finding cards…</p>}>
        <Show when={cards().length > 0} fallback={<EmptyPage page={props.page} onAdd={props.onNewCard} />}>
          <Show when={view() === "column"} fallback={
            <div class={view() === "grid" ? "grid" : "list"}>
              <Show when={props.onNewCard}>
                {(add) => (
                  <button class="tile new-tile" onClick={() => add()()}>
                    <Icon name="plus" />
                    New card
                  </button>
                )}
              </Show>
              <For each={cards()} keyed={(c) => c.id}>
                {(card) => <Tile card={card()} onClick={() => edit(card())} />}
              </For>
            </div>
          }>
            <div class="columns">
              <div class="column-list">
                <Show when={props.onNewCard}>
                  {(add) => (
                    <button class="compact new-compact" onClick={() => add()()}>
                      <Icon name="plus" />
                      New card
                    </button>
                  )}
                </Show>
                <For each={cards()} keyed={(c) => c.id}>
                  {(card) => (
                    <button
                      class={["compact", { selected: card().id === detail()?.id }]}
                      onClick={() => setSelected(card().id)}
                    >
                      <span class="compact-front">{plain(card().sides[0] ?? "") || "Empty card"}</span>
                      <DueLabel card={card()} />
                    </button>
                  )}
                </For>
              </div>
              <Show when={detail() ?? (window.matchMedia("(min-width: 700px)").matches ? cards()[0] : undefined)}>
                {(card) => <Detail card={card()} onEdit={() => edit(card())} onClose={() => setSelected(null)} />}
              </Show>
            </div>
          </Show>
        </Show>
      </Show>
    </div>
  );
}

function Detail(props: { card: CardView; onEdit: () => void; onClose: () => void }) {
  const s = () => props.card.schedule;
  return (
    <section class="detail">
      <header class="detail-head">
        <button class="icon-button only-narrow" aria-label="Back" onClick={() => props.onClose()}>
          <Icon name="back" />
        </button>
        <span class="spacer" />
        <button data-component="button-v2" data-variant="neutral" data-size="touch" onClick={() => props.onEdit()}>
          <Icon name="edit" />
          Edit card
        </button>
      </header>
      <For each={props.card.sides}>
        {(side, i) => (
          <>
            <Show when={i() > 0}>
              <hr class="side-rule" />
            </Show>
            <CardFace text={side} />
          </>
        )}
      </For>
      <div class="detail-meta">
        <span class="smallcaps">
          <Show when={s().reps > 0}>✓ {reviews(s().reps)} · </Show>
          <Show when={s().lapses > 0}>{lapses(s().lapses)} · </Show>
          <Show
            when={s().state !== "new"}
            fallback={"New, not reviewed yet"}
          >
            <Show when={s().due <= Date.now()} fallback={`Due ${until(s().due)}`}>
              <span class="accent">Due now</span>
            </Show>
          </Show>
        </span>
        <span class="tags">
          <For each={props.card.tags}>{(t) => <span class="badge">#{t}</span>}</For>
        </span>
      </div>
    </section>
  );
}

function EmptyPage(props: { page: Page; onAdd?: () => void }) {
  const [search, setSearch] = useSearchParams();
  const q = () => readText(search, "q");
  const tag = () => ("deck" in props.page ? readText(search, "tag") : "");
  const what = () =>
    [q() && `“${q()}”`, tag() && `#${tag()}`, "tag" in props.page && `#${props.page.tag}`].filter(Boolean).join(" in ");
  return (
    <Show
      when={!q() && !tag()}
      fallback={
        <div class="empty">
          <h2>No cards match {what()}</h2>
          <button
            data-component="button-v2" data-variant="neutral" data-size="touch"
            onClick={() => setSearch({ q: undefined, tag: undefined })}
          >
            {q() && tag() ? "Clear search and tag" : q() ? "Clear search" : "Show all cards"}
          </button>
        </div>
      }
    >
      <Show
        when={"deck" in props.page && props.page}
        fallback={
          <div class="empty">
            <h2>No cards tagged {what()}</h2>
          </div>
        }
      >
        {(deck) => (
          <div class="empty">
            <h2>No cards in {deck().name} yet</h2>
            <p class="muted">Write in Markdown. A line with only --- starts the next side.</p>
            <Show when={props.onAdd}>
              {(add) => (
                <button data-component="button-v2" data-variant="contrast" data-size="touch" onClick={() => add()()}>
                  Add card
                </button>
              )}
            </Show>
          </div>
        )}
      </Show>
    </Show>
  );
}

function NoDecks() {
  const app = useContext(AppContext);
  const navigate = useNavigate();
  const [name, setName] = createSignal("");
  return (
    <form
      class="empty"
      onSubmit={async (e) => {
        e.preventDefault();
        const value = name().trim();
        if (!value) return;
        const deck = await app.write((c) => c.createDeck(value, null));
        if (deck) navigate(paths.deck(deck.id));
      }}
    >
      <h2>No decks yet</h2>
      <p class="muted">Make a deck to start adding cards.</p>
      <Field
        placeholder="New deck"
        value={name()}
        onInput={(e) => setName(e.currentTarget.value)}
      />
      <button data-component="button-v2" data-variant="contrast" data-size="touch" type="submit">
        New deck
      </button>
    </form>
  );
}
