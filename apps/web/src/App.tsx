import { createRouter, useLocation, useMatch, useNavigate, useSearchParams } from "@solidjs/router";
import { createEffect, createMemo, createSignal, onSettled, Show, untrack, useContext } from "solid-js";
import type { Element } from "solid-js";
import { sameOrigin } from "./api.ts";
import { DeckScreen } from "./DeckScreen.tsx";
import { Drawer } from "./Drawer.tsx";
import { Editor } from "./Editor.tsx";
import { useOverlay } from "./nav.ts";
import { DeckCram, Review, SearchCram, TagCram } from "./Review.tsx";
import { paths, readFind, readFindCram, readText } from "./routes.ts";
import { Settings, SignIn } from "./Settings.tsx";
import { AppContext, createApp } from "./state.ts";
import { SearchLayer } from "./SearchLayer.tsx";
import { TagScreen } from "./TagScreen.tsx";
import type { CardView } from "./types.ts";

const Router = createRouter({
  routes: [
    { path: "/", component: () => <Review deck={null} /> },
    { path: "/settings", component: Settings },
    // One route for each page and what opens over it, so the page stays put under its review or cram.
    { path: ["/decks/:deck", "/decks/:deck/review", "/decks/:deck/cram"], component: DeckScreen },
    { path: ["/tags/:tag", "/tags/:tag/cram"], component: TagScreen },
    { path: "*", component: NotFound },
  ],
});

export function App() {
  return <Router>{(props) => <Root>{props.children}</Root>}</Router>;
}

/** How long the search layer takes to fade out (`.search-layer.leaving` in styles.css). */
const SEARCH_LEAVE = 180;

function NotFound() {
  useNavigate()(paths.inbox(), { replace: true });
  return null;
}

function Root(props: { children?: Element }) {
  const app = createApp();
  const navigate = useNavigate();
  const location = useLocation();

  onSettled(() => {
    // `#token=rc_...` signs in from a link (the token never reaches the server's logs that way).
    const token = new URLSearchParams(location.hash.slice(1)).get("token");
    if (token) {
      navigate(location.pathname + location.search, { replace: true });
      app.signIn({ server: sameOrigin, token });
    } else if (app.state.session) {
      void app.refresh();
    }
    // There are no push events over HTTP: refetch when the page comes back and every 30 s while it's open.
    const onVisible = () => document.visibilityState === "visible" && void app.refresh();
    document.addEventListener("visibilitychange", onVisible);
    const timer = setInterval(() => document.visibilityState === "visible" && void app.refreshDecks(), 30_000);
    return () => {
      document.removeEventListener("visibilitychange", onVisible);
      clearInterval(timer);
    };
  });

  return (
    <AppContext value={app}>
      <Show when={app.state.session} fallback={<SignIn />}>
        <Shell>{props.children}</Shell>
      </Show>
    </AppContext>
  );
}

function Shell(props: { children?: Element }) {
  const app = useContext(AppContext);
  const review = useMatch(() => "/decks/:deck/review");
  const deckCram = useMatch(() => "/decks/:deck/cram");
  const tagCram = useMatch(() => "/tags/:tag/cram");
  const deck = useMatch(() => "/decks/:deck/*rest");
  const tag = useMatch(() => "/tags/:tag");
  const [search] = useSearchParams();
  const finding = () => readFind(search) !== null;
  // The search layer stays a moment after its `find` goes (closed, or Back), to fade out over the page.
  const [layer, setLayer] = createSignal<"open" | "leaving" | null>(untrack(finding) ? "open" : null);
  let leave: ReturnType<typeof setTimeout> | undefined;
  createEffect(finding, (open) => {
    clearTimeout(leave);
    if (open) setLayer("open");
    else if (untrack(layer)) {
      setLayer("leaving");
      leave = setTimeout(() => setLayer(null), SEARCH_LEAVE);
    }
  });
  // The editor, from `?edit=<card id>` or `?edit=new` on a deck's page (or `?edit=<card id>` on a tag's, or over the
  // search), remade when that changes.
  const editing = createMemo(() => {
    const d = deck()?.params.deck ?? (tag() || finding() ? "" : undefined);
    const edit = readText(search, "edit");
    return d !== undefined && edit && (d || edit !== "new") ? JSON.stringify([d, edit]) : null;
  });

  return (
    <>
      <Drawer>
        {props.children}
        <Show when={layer()}>{(state) => <SearchLayer leaving={state() === "leaving"} />}</Show>
      </Drawer>
      <Show when={review()?.params.deck} keyed>
        {(d) => <Review deck={d} />}
      </Show>
      <Show when={deckCram()?.params.deck} keyed>
        {(d) => <DeckCram deck={d} />}
      </Show>
      <Show when={tagCram()?.params.tag} keyed>
        {(t) => <TagCram tag={t} />}
      </Show>
      <Show when={readFindCram(search)}>
        <SearchCram />
      </Show>
      <Show when={editing()} keyed>
        {(key) => {
          const [d, edit] = JSON.parse(key) as [string, string];
          return <EditorRoute deck={d} edit={edit} />;
        }}
      </Show>
      <Show when={app.state.notice}>
        {(notice) => (
          <div class="toast" role="status" onClick={() => app.setState((s) => void (s.notice = null))}>
            {notice()}
          </div>
        )}
      </Show>
    </>
  );
}

/** Opens the editor for a new card in `deck`, or for the card `edit`, fetching it when it isn't on the page. */
function EditorRoute(props: { deck: string; edit: string }) {
  const app = useContext(AppContext);
  const overlay = useOverlay();
  const { deck, edit } = untrack(() => ({ deck: props.deck, edit: props.edit }));
  const [card, setCard] = createSignal<CardView | null>(
    untrack(() => app.state.cards.find((c) => c.id === edit) ?? null),
  );
  const close = () => overlay.close(overlay.with({ edit: null }));

  if (edit !== "new" && !untrack(card)) {
    app.client()?.card(edit).then(
      (c) => setCard(() => c),
      (e) => {
        app.fail(e);
        close();
      },
    );
  }

  return (
    <Show when={edit === "new"} fallback={<Show when={card()} keyed>{(c) => <Editor editing={{ kind: "edit", card: c }} onClose={close} />}</Show>}>
      <Editor editing={{ kind: "new", deck }} onClose={close} />
    </Show>
  );
}
