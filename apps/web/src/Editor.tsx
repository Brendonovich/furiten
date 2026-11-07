import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { Compartment, EditorState, Prec } from "@codemirror/state";
import { EditorView, keymap, placeholder } from "@codemirror/view";
import { createEffect, createMemo, createSignal, createStore, flush, For, onCleanup, Show, snapshot, untrack, useContext } from "solid-js";
import { Icon } from "./Icon.tsx";
import { livePreview } from "./livePreview.ts";
import { createSheet, Sheet } from "./Sheet.tsx";
import { joinSides, normalizeTag, onReturn, pasteSides, splitSides } from "./sides.ts";
import { AppContext, deckPath, deckTree } from "./state.ts";
import type { CardView, TagSummary } from "./types.ts";

export type Editing = { kind: "new"; deck: string } | { kind: "edit"; card: CardView };

/**
 * The card editor (apps/desktop/src/editor.rs): a field for each side. A line of only `---` then Return splits a
 * side, Backspace at the start of a side joins it to the one before, pasting splits on `---`, and lists continue on
 * Return. Markdown is styled in place as you write it. Tags are chips with suggestions from the library.
 */
export function Editor(props: { editing: Editing; onClose: () => void }) {
  const app = useContext(AppContext);
  // The editor edits what it opened with; later changes to the card don't reset the form.
  const editing = untrack(() => snapshot(props.editing));
  const initial = editing.kind === "edit" ? editing.card : null;
  const [form, setForm] = createStore({
    sides: initial ? splitSides(initial.content) : [""],
    tags: initial ? [...initial.tags] : ([] as string[]),
    deck: editing.kind === "edit" ? editing.card.deck_id : editing.deck,
  });
  const [draft, setDraft] = createSignal("");
  const [confirmDelete, setConfirmDelete] = createSignal(false);
  const [saving, setSaving] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [library, setLibrary] = createSignal<TagSummary[]>([]);
  const [tagFocus, setTagFocus] = createSignal(false);
  const [highlight, setHighlight] = createSignal(0);
  const fields: EditorView[] = [];

  const sheet = createSheet();
  /** Slides the editor away; `onClose` runs once it's gone. */
  const close = () => sheet.dismiss();

  // Suggestions come from the deck the card is going into, then the rest of the library.
  createEffect(
    () => form.deck,
    (deck) => {
      void app.client()?.tags(deck).then(setLibrary, () => setLibrary([]));
    },
  );

  const suggestions = createMemo(() => {
    const have = new Set(form.tags);
    const q = normalizeTag(draft());
    const all = library().map((t) => t.tag).filter((t) => !have.has(t));
    if (!q) return all.slice(0, 8);
    const hits = all.filter((t) => t.includes(q));
    return [...hits.filter((t) => t.startsWith(q)), ...hits.filter((t) => !t.startsWith(q))];
  });

  const addTag = (raw: string) => {
    const tag = normalizeTag(raw);
    setDraft("");
    setHighlight(0);
    if (tag && !form.tags.includes(tag)) setForm((f) => void f.tags.push(tag));
  };

  /** Puts the caret in side `i` at `at` once the DOM has the new sides. */
  const focusAt = (i: number, at: number) => {
    flush();
    const view = fields[i];
    if (!view) return;
    setDoc(view, form.sides[i] ?? "");
    view.focus();
    view.dispatch({ selection: { anchor: Math.min(at, view.state.doc.length) }, scrollIntoView: true });
  };

  /** A key in side `i`'s field; `true` when the editor handled it. */
  const onKey = (i: number, key: "Return" | "Save" | "Backspace" | "Up" | "Down", view: EditorView): boolean => {
    const { from: start, to: end } = view.state.selection.main;
    const value = view.state.doc.toString();
    if (key === "Save") {
      void save();
      return true;
    }
    if (key === "Return" && start === end) {
      const action = onReturn(value, start);
      if (!action) return false;
      if (action.kind === "split") {
        setForm((f) => void f.sides.splice(i, 1, action.before, action.after));
        focusAt(i + 1, 0);
      } else {
        setForm((f) => void (f.sides[i] = action.text));
        focusAt(i, action.caret);
      }
      return true;
    }
    if (key === "Backspace" && i > 0 && start === 0 && end === 0) {
      const prev = form.sides[i - 1] ?? "";
      const joined = prev && value ? `${prev}\n${value}` : prev + value;
      setForm((f) => void f.sides.splice(i - 1, 2, joined));
      focusAt(i - 1, prev.length + (prev && value ? 1 : 0));
      return true;
    }
    if (key === "Up" && i > 0 && start === 0 && end === 0) {
      focusAt(i - 1, (form.sides[i - 1] ?? "").length);
      return true;
    }
    if (key === "Down" && i < form.sides.length - 1 && start === value.length) {
      focusAt(i + 1, 0);
      return true;
    }
    return false;
  };

  const onPaste = (i: number, e: ClipboardEvent, view: EditorView): boolean => {
    const pasted = e.clipboardData?.getData("text/plain") ?? "";
    const { from, to } = view.state.selection.main;
    const sides = pasteSides(view.state.doc.toString(), from, to, pasted);
    if (!sides) return false;
    e.preventDefault();
    setForm((f) => void f.sides.splice(i, 1, ...sides));
    focusAt(i + sides.length - 1, (sides[sides.length - 1] ?? "").length);
    return true;
  };

  const addSide = () => {
    const last = form.sides.length - 1;
    if ((form.sides[last] ?? "").trim() === "") return focusAt(last, 0);
    setForm((f) => void f.sides.push(""));
    focusAt(last + 1, 0);
  };

  const save = async () => {
    const content = joinSides(form.sides);
    if (!content.trim()) return setError("Write something on the card first");
    const tags = draft().trim() ? [...form.tags, normalizeTag(draft())] : [...form.tags];
    setSaving(true);
    const card = { content, tags, deck_id: form.deck };
    const done = await app.write((c) => (initial ? c.updateCard(initial.id, card) : c.createCard(card)));
    setSaving(false);
    if (done) close();
  };

  const remove = async () => {
    if (!initial) return;
    if (!confirmDelete()) return setConfirmDelete(true);
    await app.write((c) => c.deleteCard(initial.id));
    close();
  };

  return (
    <Sheet
      title={initial ? "Edit card" : "New card"}
      onClose={() => props.onClose()}
      sheet={sheet}
      tall
      initialFocus={() => (initial ? undefined : fields[0]?.contentDOM)}
    >
      {/* Delete asks once more before it deletes. */}
      <header class="editor-head">
        <Show when={initial}>
          <button
            data-component="button-v2"
            data-variant={confirmDelete() ? "danger" : "ghost-muted"}
            data-size="large"
            class="editor-delete"
            onClick={() => void remove()}
            onBlur={() => setConfirmDelete(false)}
          >
            <Icon name="trash" />
            {confirmDelete() ? "Delete this card" : "Delete"}
          </button>
        </Show>
        <button
          data-component="button-v2"
          data-variant="neutral"
          data-size="large"
          class="editor-save"
          disabled={saving()}
          onClick={() => void save()}
        >
          Save
        </button>
      </header>

      <div class="editor-body">
        {/* A tap on the card's empty space writes at the end of the last side. */}
        <div class="well" onClick={(e) => e.target === e.currentTarget && focusAt(form.sides.length - 1, Infinity)}>
          <For each={form.sides} keyed={false}>
            {(side, i) => (
              <>
                <Show when={i > 0}>
                  <hr class="side-rule" />
                </Show>
                <Side
                  value={side()}
                  placeholder={form.sides.length === 1 ? "Write in Markdown" : "Empty side"}
                  ref={(view) => (fields[i] = view)}
                  onInput={(v) => setForm((f) => void (f.sides[i] = v))}
                  onKey={(key, view) => onKey(i, key, view)}
                  onPaste={(e, view) => onPaste(i, e, view)}
                />
              </>
            )}
          </For>
          <div class="well-foot">
            <button data-component="button-v2" data-variant="outline" data-size="large" data-icon class="add-side" onClick={addSide}>
              <Icon name="plus" /> Side
            </button>
            <span class="faint small">or type --- on its own line</span>
          </div>
        </div>

        <Show when={tagFocus() && suggestions().length > 0}>
          <div class="suggestions" data-scroll-x>
            <For each={suggestions()}>
              {(tag, i) => (
                <button
                  class={["chip", { active: i() === highlight() && !!draft() }]}
                  onPointerDown={(e) => e.preventDefault()}
                  onClick={() => addTag(tag)}
                >
                  #{tag}
                </button>
              )}
            </For>
          </div>
        </Show>

        <Show when={error()}>{(e) => <p class="error">{e()}</p>}</Show>

        <div class="editor-meta">
          <label class="select deck-select">
            <Icon name="deck" />
            <span>{deckPath(app.state.decks, form.deck)}</span>
            <Icon name="chevron" />
            <select
              aria-label="Deck"
              value={form.deck}
              onChange={(e) => setForm((f) => void (f.deck = e.currentTarget.value))}
            >
              <For each={deckTree(app.state.decks)} keyed={(t) => t.deck.id}>
                {(t) => <option value={t().deck.id}>{deckPath(app.state.decks, t().deck.id)}</option>}
              </For>
            </select>
          </label>
          <For each={form.tags}>
            {(tag) => (
              <span class="chip-tag">
                #{tag}
                <button
                  aria-label={`Remove ${tag}`}
                  onClick={() => setForm((f) => void f.tags.splice(f.tags.indexOf(tag), 1))}
                >
                  <Icon name="close" />
                </button>
              </span>
            )}
          </For>
          <label class="tag-add">
            <Icon name="plus" />
            <input
              aria-label="Add tag"
              placeholder="Tag"
              autocapitalize="none"
              autocomplete="off"
              enterkeyhint="done"
              value={draft()}
              onFocus={() => setTagFocus(true)}
              onBlur={() => setTimeout(() => setTagFocus(false), 150)}
              onInput={(e) => {
                const v = e.currentTarget.value;
                if (/[,\s]/.test(v)) {
                  for (const part of v.split(/[,\s]+/).slice(0, -1)) addTag(part);
                  const rest = v.split(/[,\s]+/).at(-1) ?? "";
                  e.currentTarget.value = rest;
                  setDraft(rest);
                } else {
                  setDraft(v);
                  setHighlight(0);
                }
              }}
              onKeyDown={(e) => {
                const list = suggestions();
                if (e.key === "Enter") {
                  e.preventDefault();
                  const pick = draft() ? (list[highlight()] ?? draft()) : list[highlight()];
                  if (pick) addTag(pick);
                  e.currentTarget.value = "";
                } else if (e.key === "Backspace" && !e.currentTarget.value && form.tags.length) {
                  setForm((f) => void f.tags.pop());
                } else if (e.key === "ArrowDown" && list.length) {
                  e.preventDefault();
                  setHighlight((highlight() + 1) % list.length);
                } else if (e.key === "ArrowUp" && list.length) {
                  e.preventDefault();
                  setHighlight((highlight() - 1 + list.length) % list.length);
                }
              }}
            />
          </label>
        </div>
      </div>

    </Sheet>
  );
}

/** Replaces `view`'s text with `text`, changing only what differs so the caret and undo stay put. */
function setDoc(view: EditorView, text: string) {
  const doc = view.state.doc.toString();
  if (doc === text) return;
  let from = 0;
  while (from < doc.length && from < text.length && doc[from] === text[from]) from++;
  let end = 0;
  while (end < doc.length - from && end < text.length - from && doc[doc.length - 1 - end] === text[text.length - 1 - end]) end++;
  view.dispatch({ changes: { from, to: doc.length - end, insert: text.slice(from, text.length - end) } });
}

type Key = "Return" | "Save" | "Backspace" | "Up" | "Down";

/** One side's field: Markdown styled in place (livePreview.ts), in CodeMirror. */
function Side(props: {
  value: string;
  placeholder: string;
  ref: (view: EditorView) => void;
  onInput: (value: string) => void;
  onKey: (key: Key, view: EditorView) => boolean;
  onPaste: (e: ClipboardEvent, view: EditorView) => boolean;
}) {
  const hint = new Compartment();
  const bind = (key: string, name: Key) => ({ key, run: (view: EditorView) => props.onKey(name, view) });
  // iOS's keyboard reports Shift with Return and Backspace while it's capitalizing, as it is at the start of a side.
  const bindShifted = (key: string, name: Key) => ({ ...bind(key, name), shift: (view: EditorView) => props.onKey(name, view) });
  const view = new EditorView({
    state: EditorState.create({
      doc: untrack(() => props.value),
      extensions: [
        Prec.highest(
          keymap.of([
            bind("Mod-Enter", "Save"),
            bindShifted("Enter", "Return"),
            bindShifted("Backspace", "Backspace"),
            bind("ArrowUp", "Up"),
            bind("ArrowDown", "Down"),
          ]),
        ),
        Prec.highest(EditorView.domEventHandlers({ paste: (e, view) => props.onPaste(e, view) })),
        history(),
        keymap.of([...defaultKeymap, ...historyKeymap]),
        EditorView.lineWrapping,
        EditorView.contentAttributes.of({ autocapitalize: "sentences", "aria-multiline": "true" }),
        hint.of(placeholder(untrack(() => props.placeholder))),
        livePreview(),
        EditorView.updateListener.of((u) => {
          if (u.docChanged) props.onInput(u.state.doc.toString());
        }),
      ],
    }),
  });
  props.ref(view);
  onCleanup(() => view.destroy());

  createEffect(
    () => props.value,
    (value) => setDoc(view, value),
  );
  createEffect(
    () => props.placeholder,
    (text) => view.dispatch({ effects: hint.reconfigure(placeholder(text)) }),
  );

  return <div class="side-field">{view.dom}</div>;
}
