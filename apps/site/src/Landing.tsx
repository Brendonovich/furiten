import { createMemo, createSignal, For, onCleanup, Show } from "solid-js";
import { CardSides } from "../../web/src/Card.tsx";
import { CARDS } from "./cards.ts";
import { advance, dueIn, dueLabel, OPEN, showing, type Rotation } from "./schedule.ts";

const REPO = "https://github.com/Brendonovich/furiten";
/** The current Mac DMG, from Anomaly's release service (apps/desktop/README.md). */
const DOWNLOAD_MAC = "https://anoma.ly/release/furiten/desktop/stable/macos-arm64-dmg";
/** How often the rotation moves on. */
const TICK = 100;
/** How many columns of cards fit, widest first. */
const COLUMNS = [
  { query: "(min-width: 1000px)", n: 3 },
  { query: "(min-width: 640px)", n: 2 },
];

export function Landing() {
  return (
    <main class="landing">
      <header class="hero">
        <img class="hero-icon" src="/icon.svg" alt="" width="64" height="64" />
        <h1>Flashcards that know when you’ll forget.</h1>
        <p class="lede">
          Write cards in Markdown and review them with spaced repetition. Furiten works offline, syncs across your
          devices, and your agents can use it too.
        </p>
        <div class="hero-actions">
          <a data-component="button-v2" data-variant="contrast" data-size="touch" href={DOWNLOAD_MAC}>
            Download for Mac
          </a>
          <a data-component="button-v2" data-variant="neutral" data-size="touch" href={REPO}>
            View on GitHub
          </a>
        </div>
      </header>
      <Benefits />
      <footer class="site-footer faint">Furiten</footer>
    </main>
  );
}

/** How many columns the window has room for, kept up to date as it resizes. */
function columnCount() {
  const queries = COLUMNS.map((c) => ({ ...c, list: matchMedia(c.query) }));
  const count = () => queries.find((q) => q.list.matches)?.n ?? 1;
  const [n, setN] = createSignal(count());
  const update = () => setN(count());
  for (const q of queries) q.list.addEventListener("change", update);
  onCleanup(() => queries.forEach((q) => q.list.removeEventListener("change", update)));
  return n;
}

/**
 * The benefits as review cards, laid out in columns so a card growing as it opens only moves the cards under it.
 * Each takes its turn opening, one after another, and goes back to its front when the turn ends, coming in again as
 * a card does in review. Tap one to reveal its next side and hold it open, so the rotation skips it until it's
 * tapped closed. Hovering the cards or hiding the page pauses the rotation, and it starts paused for anyone who'd
 * rather things didn't move on their own.
 */
function Benefits() {
  const [held, setHeld] = createSignal<boolean[]>(CARDS.map(() => false));
  /** Bumped each time a card closes, so it comes in afresh. */
  const [rounds, setRounds] = createSignal<number[]>(CARDS.map(() => 1));
  const [rotation, setRotation] = createSignal<Rotation>({ turn: 0, elapsed: 0 });
  const [playing, setPlaying] = createSignal(!matchMedia("(prefers-reduced-motion: reduce)").matches);
  const [hovering, setHovering] = createSignal(false);
  const [hidden, setHidden] = createSignal(document.hidden);
  const columns = columnCount();

  const close = (i: number) => setRounds((r) => r.map((n, j) => (j === i ? n + 1 : n)));

  const onVisibility = () => setHidden(document.hidden);
  document.addEventListener("visibilitychange", onVisibility);
  const timer = setInterval(() => {
    if (!playing() || hovering() || hidden()) return;
    const before = rotation();
    const after = advance(before, held(), TICK);
    setRotation(after);
    CARDS.forEach((_, i) => {
      if (showing(before, i) && !showing(after, i) && !held()[i]) close(i);
    });
  }, TICK);
  onCleanup(() => {
    clearInterval(timer);
    document.removeEventListener("visibilitychange", onVisibility);
  });

  const toggle = (i: number) => {
    const r = rotation();
    if (held()[i]) {
      setHeld((h) => h.map((v, j) => (j === i ? false : v)));
      close(i);
      return;
    }
    // Holding the card that's open on its turn ends its turn, so the next one doesn't wait.
    if (showing(r, i)) setRotation({ turn: i, elapsed: OPEN });
    setHeld((h) => h.map((v, j) => (j === i ? true : v)));
  };

  const card = (i: number) => {
    const sides = CARDS[i] ?? [];
    const open = createMemo(() => held()[i] || showing(rotation(), i));
    const due = createMemo(() => dueIn(rotation(), held(), i));
    return (
      <div class="benefit">
        <p class="benefit-due smallcaps">
          <Show when={held()[i]} fallback={<span class={{ accent: due() === 0 }}>{due() === null ? "" : dueLabel(due()!)}</span>}>
            Tap to hide
          </Show>
        </p>
        {/* Keyed on its round, so a card that closes comes in again as review brings in the next card. */}
        <Show when={rounds()[i]} keyed>
          {(_round) => (
            <div
              class="review-card"
              role="button"
              tabindex="0"
              aria-expanded={open() ? "true" : "false"}
              onClick={() => toggle(i)}
              onKeyDown={(e) => {
                if (e.key !== "Enter" && e.key !== " ") return;
                e.preventDefault();
                toggle(i);
              }}
            >
              <CardSides sides={sides} shown={open() ? sides.length : 1} />
            </div>
          )}
        </Show>
      </div>
    );
  };

  return (
    <section class="benefits" aria-label="What Furiten does">
      <div class="benefits-bar">
        <p class="faint">Tap a card to reveal the next side.</p>
        <button
          data-component="button-v2" data-variant="ghost" data-size="touch"
          onClick={() => setPlaying((p) => !p)}
        >
          {playing() ? "Pause" : "Play"}
        </button>
      </div>
      <div
        class="benefit-columns"
        onPointerEnter={(e) => e.pointerType === "mouse" && setHovering(true)}
        onPointerLeave={() => setHovering(false)}
      >
        <For each={Array.from({ length: columns() }, (_, c) => c)}>
          {(c) => (
            <div class="benefit-column">
              <For each={CARDS.map((_, i) => i).filter((i) => i % columns() === c)}>{(i) => card(i)}</For>
            </div>
          )}
        </For>
      </div>
    </section>
  );
}
