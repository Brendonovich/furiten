import type { JSX } from "@solidjs/web";
import { type Element, omit, Show } from "solid-js";

/**
 * A text field in OpenCode's `text-input-v2` markup, so it takes that component's styles: the surface, input
 * elevation, focus outline and placeholder colour. `appearance="touch"` is this app's phone-sized variant.
 */
export function Field(
  props: JSX.InputHTMLAttributes<HTMLInputElement> & { mono?: boolean; leading?: Element; trailing?: Element },
) {
  const input = omit(props, "mono", "leading", "trailing", "class");
  return (
    <div
      data-component="text-input-v2"
      data-appearance="touch"
      data-surface="base"
      data-leading-icon={props.leading ? "" : undefined}
      class={["field", props.class]}
    >
      <div data-slot="text-input-v2-value">
        <Show when={props.leading}>
          <span data-slot="text-input-v2-leading-icon">{props.leading}</span>
        </Show>
        <input {...input} data-slot="text-input-v2-input" class={{ mono: !!props.mono }} />
      </div>
      <Show when={props.trailing}>
        <div data-slot="text-input-v2-trailing">{props.trailing}</div>
      </Show>
    </div>
  );
}
