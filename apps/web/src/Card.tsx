import { createMemo, Show } from "solid-js";
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
