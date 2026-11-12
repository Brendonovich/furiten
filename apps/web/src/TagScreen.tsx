import { useParams } from "@solidjs/router";
import { useContext } from "solid-js";
import { Cards, type Page, PageFooter, Tools, usePageCards } from "./DeckScreen.tsx";
import { Icon } from "./Icon.tsx";
import { AppContext } from "./state.ts";

/**
 * Every card with a tag, from every deck, `/tags/:tag`: a deck's page but for the tag filter and New card, with the same
 * view, sort and search in the URL; tapping a card edits it, and Review crams what's listed.
 */
export function TagScreen() {
  const app = useContext(AppContext);
  const params = useParams<{ tag: string }>();
  const tag = () => params.tag ?? "";
  const page = (): Page => ({ tag: tag() });
  usePageCards(page);

  return (
    <div class="screen">
      <header class="toolbar">
        <button class="icon-button" aria-label="Decks" onClick={() => app.setState((s) => void (s.drawer = true))}>
          <Icon name="menu" />
        </button>
        <h1 class="toolbar-title">#{tag()}</h1>
        <span class="icon-button toolbar-balance" aria-hidden="true" />
      </header>
      <Tools page={page()} />
      <Cards page={page()} />
      <PageFooter page={page()} />
    </div>
  );
}
