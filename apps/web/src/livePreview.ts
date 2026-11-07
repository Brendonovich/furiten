import { type Extension, RangeSet, type Range as Ranged } from "@codemirror/state";
import { Decoration, type DecorationSet, EditorView, ViewPlugin, type ViewUpdate, WidgetType } from "@codemirror/view";
import { markup, shownLines } from "./markup.ts";

class Bullet extends WidgetType {
  override eq() {
    return true;
  }
  toDOM() {
    const el = document.createElement("span");
    el.className = "cm-bullet";
    el.textContent = "•";
    return el;
  }
}

const bullet = Decoration.replace({ widget: new Bullet() });
const hide = Decoration.replace({});
const fade = Decoration.mark({ class: "cm-md-mark" });
const styles = new Map(
  ["strong", "em", "strike", "code", "link", "url", "quote"].map((s) => [s, Decoration.mark({ class: `cm-md-${s}` })]),
);
const lines = new Map(
  ["h1", "h2", "h3", "h4", "code"].map((s) => [s, Decoration.line({ class: `cm-md-line-${s}` })]),
);

function build(view: EditorView): { decorations: DecorationSet; atomic: RangeSet<Decoration> } {
  const src = view.state.doc.toString();
  const sel = view.state.selection.main;
  const shown = view.hasFocus ? shownLines(src, sel.from, sel.to) : null;
  const decos: Ranged<Decoration>[] = [];
  const atomic: Ranged<Decoration>[] = [];
  for (const m of markup(src, shown)) {
    switch (m.kind) {
      case "style":
        decos.push(styles.get(m.style)!.range(m.from, m.to));
        break;
      case "line":
        decos.push(lines.get(m.style)!.range(m.at));
        break;
      case "fade":
        decos.push(fade.range(m.from, m.to));
        break;
      case "hide":
        decos.push(hide.range(m.from, m.to));
        break;
      case "bullet": {
        // Indented to its nesting, with wrapped lines hanging under the text rather than the bullet.
        const line = Decoration.line({ class: "cm-md-line-bullet", attributes: { style: `--depth: ${m.depth}` } });
        decos.push(line.range(m.from), bullet.range(m.from, m.to));
        atomic.push(bullet.range(m.from, m.to));
        break;
      }
    }
  }
  return { decorations: Decoration.set(decos, true), atomic: Decoration.set(atomic, true) };
}

/** Markdown styled in place, its marks shown only on the caret's lines (see markup.ts). */
export function livePreview(): Extension {
  const plugin = ViewPlugin.fromClass(
    class {
      decorations: DecorationSet;
      atomic: RangeSet<Decoration>;
      constructor(view: EditorView) {
        ({ decorations: this.decorations, atomic: this.atomic } = build(view));
      }
      update(u: ViewUpdate) {
        if (u.docChanged || u.selectionSet || u.focusChanged) {
          ({ decorations: this.decorations, atomic: this.atomic } = build(u.view));
        }
      }
    },
    { decorations: (v) => v.decorations },
  );
  // The caret steps over a bullet's hidden marker, so it never goes where nothing is drawn.
  return [plugin, EditorView.atomicRanges.of((view) => view.plugin(plugin)?.atomic ?? RangeSet.empty)];
}
