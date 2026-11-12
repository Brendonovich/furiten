import { useLocation, useSearchParams } from "@solidjs/router";
import { createEffect, createMemo, createSignal, For, onSettled, Show, untrack, useContext } from "solid-js";
import { Tile } from "./Card.tsx";
import { liftOverKeyboard } from "./DeckScreen.tsx";
import { Field } from "./Field.tsx";
import { Icon } from "./Icon.tsx";
import { useOverlay } from "./nav.ts";
import { CramButton } from "./Review.tsx";
import { readFind, searchCards, withFind } from "./routes.ts";
import { AppContext, byDeck } from "./state.ts";
import type { CardView } from "./types.ts";

/**
 * Searching every deck's cards, a layer over whichever page is open (`?find=`, routes.ts): the matches as tiles under
 * their deck's name, most recently updated first, with the field at its foot as on a deck's page. Tapping a card edits
 * it; Close or Escape drops the search, back to the page as it was. With `leaving`, it's on its way out.
 */
export function SearchLayer(props: { leaving: boolean }) {
  const app = useContext(AppContext);
  const overlay = useOverlay();
  const location = useLocation();
  const [search] = useSearchParams();
  const url = () => location.pathname + location.search;
  // Kept while leaving, so the layer fades out as it was rather than emptying first.
  const [find, setFind] = createSignal(untrack(() => readFind(search) ?? ""));
  createEffect(
    () => readFind(search),
    (value) => void (value !== null && setFind(value)),
  );
  const q = () => find().trim();
  const [results, setResults] = createSignal<{ q: string; cards: CardView[] } | null>(null);

  // Refetched with the decks, after every write and whenever the page comes back.
  createEffect(
    () => [q(), app.state.lastSync] as const,
    ([query]) => {
      if (!query) return;
      void app.client()?.cards(searchCards(query)).then(
        (cards) => query === q() && setResults({ q: query, cards }),
        (e) => app.fail(e),
      );
    },
  );
  const shown = () => (results()?.q === q() ? results()!.cards : null);

  /** The matches grouped by deck, in the sidebar's order. */
  const groups = createMemo(() => byDeck(app.state.decks, shown() ?? []));
  const [focused, setFocused] = createSignal(false);

  // The field leads the URL while you type; Back sets it from the URL.
  let timer: ReturnType<typeof setTimeout> | undefined;
  let input: HTMLInputElement | undefined;
  const [draft, setDraft] = createSignal(untrack(find));
  let written: string | null = null;
  createEffect(find, (value) => {
    if (value === written) return;
    written = null;
    setDraft(value);
  });
  const commit = (value: string) => {
    clearTimeout(timer);
    if (props.leaving) return;
    written = value;
    overlay.replace(withFind(url(), value));
  };
  const close = () => {
    clearTimeout(timer);
    input?.blur();
    overlay.close(withFind(url(), null));
  };

  let footer!: HTMLElement;
  onSettled(() => {
    input?.focus();
    return liftOverKeyboard(footer);
  });

  return (
    <div class={["search-layer", { leaving: props.leaving }]} inert={props.leaving || undefined}>
      <header class="toolbar">
        <button class="icon-button" aria-label="Decks" onClick={() => app.setState((s) => void (s.drawer = true))}>
          <Icon name="menu" />
        </button>
        <h1 class="toolbar-title">Search</h1>
        <span class="icon-button toolbar-balance" aria-hidden="true" />
      </header>
      <div class="cards-scroll">
        <Show
          when={q()}
          fallback={
            <div class="empty">
              <h2>Search cards</h2>
              <p class="faint">Find cards in every deck.</p>
            </div>
          }
        >
          <Show when={shown()} fallback={<p class="center faint">Searching…</p>}>
            {(cards) => (
              <Show
                when={cards().length > 0}
                fallback={
                  <div class="empty">
                    <h2>No results for “{q()}”</h2>
                    <p class="faint">Check the spelling or try a new search.</p>
                  </div>
                }
              >
                <For each={groups()} keyed={(g) => g.deck.id}>
                  {(group) => (
                    <section class="search-group">
                      <h2 class="section-title">{group().deck.name}</h2>
                      <div class="grid">
                        <For each={group().cards} keyed={(c) => c.id}>
                          {(card) => (
                            <Tile card={card()} onClick={() => overlay.open(overlay.with({ edit: card().id }))} />
                          )}
                        </For>
                      </div>
                    </section>
                  )}
                </For>
              </Show>
            )}
          </Show>
        </Show>
      </div>
      <footer ref={footer} class="deck-footer searching">
        <Field
          ref={(el) => (input = el)}
          class="search"
          type="search"
          placeholder="Search cards in every deck"
          aria-label="Search cards in every deck"
          enterkeyhint="search"
          leading={<Icon name="search" />}
          value={draft()}
          onFocus={() => setFocused(true)}
          onBlur={() => setFocused(false)}
          onInput={(e) => {
            const value = e.currentTarget.value;
            setDraft(value);
            clearTimeout(timer);
            timer = setTimeout(() => commit(value), 150);
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
        {/* Out of the way while typing; back between the field and Close once the search is in. */}
        <Show when={!focused()}>
          <CramButton cards={shown() && groups().flatMap((g) => g.cards)} at={withFind(url(), find(), true)} />
        </Show>
        <button
          data-component="button-v2" data-variant="neutral" data-size="touch" class="fab" aria-label="Close search"
          // Before the field's blur, so a tap here closes the search rather than only the keyboard.
          onPointerDown={(e) => e.preventDefault()}
          onClick={close}
        >
          <Icon name="close" />
        </button>
      </footer>
    </div>
  );
}
