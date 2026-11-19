import { createMemo, For, Show } from "solid-js";
import { frontSize, hiddenSides, untilShort } from "./format.ts";
import { renderMarkdown } from "./markdown.ts";
import type { CardView } from "./types.ts";

/** One side as rendered Markdown. With `size`, short fronts render big, as tiles do on the desktop. */
export function CardFace(props: { text: string; size?: boolean }) {
  const html = createMemo(() => renderMarkdown(props.text));
  return (
    <div
      class={["markdown", props.size ? `size-${frontSize(props.text)}` : undefined]}
      innerHTML={html()}
    />
  );
}

/**
 * A card's sides as review shows them, inside a `.review-card`: the first `shown`, split by dashed rules, each side
 * after the first animating in as it's revealed, then its tags once every side shows.
 */
export function CardSides(props: { sides: readonly string[]; shown: number; tags?: readonly string[] }) {
  return (
    <>
      <For each={props.sides.slice(0, props.shown)}>
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
      <Show when={props.shown >= props.sides.length && props.tags?.length}>
        <div class={["tags review-tags", { "reveal-in": props.sides.length > 1 }]}>
          <For each={props.tags}>{(t) => <span class="badge">#{t}</span>}</For>
        </div>
      </Show>
    </>
  );
}

/** A card as a tile: its front, how many sides it hides, and when it's due. */
export function Tile(props: { card: CardView; onClick: () => void }) {
  return (
    <button class="tile" onClick={() => props.onClick()}>
      <CardFace text={props.card.sides[0] ?? ""} size />
      <span class="tile-footer">
        <span>
          <Show when={props.card.sides.length > 1}>+ {hiddenSides(props.card.sides.length - 1)}</Show>
        </span>
        <DueLabel card={props.card} />
      </span>
    </button>
  );
}

/** "NEW", "DUE" (accent), or how long until it's due: "3 DAYS". */
export function DueLabel(props: { card: CardView }) {
  return (
    <Show when={props.card.schedule.state !== "new"} fallback={<span class="smallcaps">New</span>}>
      <Show
        when={props.card.schedule.due <= Date.now()}
        fallback={<span class="smallcaps">{untilShort(props.card.schedule.due)}</span>}
      >
        <span class="smallcaps accent">Due</span>
      </Show>
    </Show>
  );
}
