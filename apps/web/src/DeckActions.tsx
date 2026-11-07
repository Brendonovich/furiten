import { useNavigate } from "@solidjs/router";
import { createMemo, createSignal, For, Match, Switch, untrack, useContext } from "solid-js";
import { Field } from "./Field.tsx";
import { Icon } from "./Icon.tsx";
import { paths, type DeckStep } from "./routes.ts";
import { createSheet, Sheet } from "./Sheet.tsx";
import { AppContext, deckPath, deckTree } from "./state.ts";
import type { DeckSummary } from "./types.ts";

/** Renaming, nesting, moving and deleting a deck, at the step the URL says (`?actions=rename`). */
export function DeckActions(props: {
  deck: DeckSummary;
  step: DeckStep;
  onStep: (step: DeckStep) => void;
  onClose: () => void;
}) {
  const app = useContext(AppContext);
  const navigate = useNavigate();
  const step = () => props.step;
  const setStep = (step: DeckStep) => props.onStep(step);
  const [name, setName] = createSignal(untrack(() => (props.step === "rename" ? props.deck.name : "")));
  const [busy, setBusy] = createSignal(false);
  const sheet = createSheet();

  /** Where it can move: the top level, or any deck that isn't it or under it. */
  const targets = createMemo(() => {
    const under = new Set([props.deck.id]);
    for (const { deck } of deckTree(app.state.decks)) if (deck.parent_id && under.has(deck.parent_id)) under.add(deck.id);
    return deckTree(app.state.decks).filter(({ deck }) => !under.has(deck.id));
  });

  const run = async (f: () => Promise<unknown>) => {
    setBusy(true);
    await f();
    setBusy(false);
    sheet.dismiss();
  };

  return (
    <Sheet title={props.deck.name} onClose={props.onClose} sheet={sheet}>
      <Switch>
        <Match when={step() === "menu"}>
          <h2 class="sheet-title">{props.deck.name}</h2>
          <p class="faint sheet-detail">{props.deck.cards} cards</p>
          <div class="action-list">
            <button
              onClick={() => {
                setName(props.deck.name);
                setStep("rename");
              }}
            >
              <Icon name="edit" /> Rename
            </button>
            <button
              onClick={() => {
                setName("");
                setStep("subdeck");
              }}
            >
              <Icon name="subdeck" /> New subdeck
            </button>
            <button onClick={() => setStep("move")}>
              <Icon name="deck" /> Move to…
            </button>
            <hr class="menu-rule" />
            <button class="danger" onClick={() => setStep("delete")}>
              <Icon name="trash" /> Delete deck
            </button>
          </div>
        </Match>
        <Match when={step() === "rename" || step() === "subdeck"}>
          <form
            class="sheet-form"
            onSubmit={(e) => {
              e.preventDefault();
              const value = name().trim();
              if (!value) return;
              void run(async () => {
                if (step() === "rename") {
                  await app.write((c) => c.updateDeck(props.deck.id, { name: value }));
                } else {
                  const deck = await app.write((c) => c.createDeck(value, props.deck.id));
                  if (deck) navigate(paths.deck(deck.id), { replace: true });
                }
              });
            }}
          >
            <h2 class="sheet-title">{step() === "rename" ? "Rename deck" : `New deck in ${props.deck.name}`}</h2>
            <Field
              ref={(el) => queueMicrotask(() => el.focus())}
              placeholder="Name"
              enterkeyhint="done"
              value={name()}
              onInput={(e) => setName(e.currentTarget.value)}
            />
            <div class="sheet-buttons">
              <button type="button" data-component="button-v2" data-variant="neutral" data-size="touch" onClick={() => sheet.dismiss()}>
                Cancel
              </button>
              <button type="submit" data-component="button-v2" data-variant="contrast" data-size="touch" disabled={busy() || !name().trim()}>
                Save
              </button>
            </div>
          </form>
        </Match>
        <Match when={step() === "move"}>
          <h2 class="sheet-title">Move {props.deck.name}</h2>
          <div class="action-list scroll">
            <button
              class={{ selected: props.deck.parent_id === null }}
              onClick={() => void run(() => app.write((c) => c.updateDeck(props.deck.id, { parent_id: null })))}
            >
              <Icon name="deck" /> Top level
            </button>
            <For each={targets()} keyed={(t) => t.deck.id}>
              {(t) => (
                <button
                  class={{ selected: props.deck.parent_id === t().deck.id }}
                  style={{ "padding-left": `${16 + t().depth * 16}px` }}
                  onClick={() =>
                    void run(() => app.write((c) => c.updateDeck(props.deck.id, { parent_id: t().deck.id })))
                  }
                >
                  <Icon name={t().depth ? "subdeck" : "deck"} /> {deckPath(app.state.decks, t().deck.id)}
                </button>
              )}
            </For>
          </div>
        </Match>
        <Match when={step() === "delete"}>
          <h2 class="sheet-title">Delete {props.deck.name}?</h2>
          <p class="muted sheet-detail">Its subdecks and all their cards are deleted too.</p>
          <div class="sheet-buttons">
            <button data-component="button-v2" data-variant="neutral" data-size="touch" onClick={() => sheet.dismiss()}>
              Cancel
            </button>
            <button
              data-component="button-v2" data-variant="danger" data-size="touch"
              disabled={busy()}
              onClick={() =>
                void run(async () => {
                  const parent = props.deck.parent_id;
                  if (!(await app.write((c) => c.deleteDeck(props.deck.id)))) return;
                  navigate(parent ? paths.deck(parent) : paths.inbox(), { replace: true });
                })
              }
            >
              Delete deck and its cards
            </button>
          </div>
        </Match>
      </Switch>
    </Sheet>
  );
}
