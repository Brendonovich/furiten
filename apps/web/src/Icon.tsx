// Small line icons, drawn at 20px with the current text colour. The drawings themselves live in the shared
// `assets/icons/*.svg` (the one set the desktop and iOS apps draw too); this component only imports their path
// data and stroke width, then strokes them in `currentColor` with round caps and joins, fill none.
import back from "../../../assets/icons/back.svg?raw";
import check from "../../../assets/icons/check.svg?raw";
import chevron from "../../../assets/icons/chevron.svg?raw";
import clock from "../../../assets/icons/clock.svg?raw";
import close from "../../../assets/icons/close.svg?raw";
import cloud from "../../../assets/icons/cloud.svg?raw";
import column from "../../../assets/icons/column.svg?raw";
import deck from "../../../assets/icons/deck.svg?raw";
import down from "../../../assets/icons/down.svg?raw";
import edit from "../../../assets/icons/edit.svg?raw";
import eye from "../../../assets/icons/eye.svg?raw";
import filter from "../../../assets/icons/filter.svg?raw";
import grid from "../../../assets/icons/grid.svg?raw";
import inbox from "../../../assets/icons/inbox.svg?raw";
import list from "../../../assets/icons/list.svg?raw";
import menu from "../../../assets/icons/menu.svg?raw";
import more from "../../../assets/icons/more.svg?raw";
import order from "../../../assets/icons/order.svg?raw";
import play from "../../../assets/icons/play.svg?raw";
import plus from "../../../assets/icons/plus.svg?raw";
import search from "../../../assets/icons/search.svg?raw";
import settings from "../../../assets/icons/settings.svg?raw";
import shuffle from "../../../assets/icons/shuffle.svg?raw";
import sort from "../../../assets/icons/sort.svg?raw";
import subdeck from "../../../assets/icons/subdeck.svg?raw";
import tag from "../../../assets/icons/tag.svg?raw";
import trash from "../../../assets/icons/trash.svg?raw";
import up from "../../../assets/icons/up.svg?raw";

// The path and stroke width from one of the shared SVGs (a single `<path>` element each).
const parse = (svg: string) => ({
  d: / d="([^"]*)"/.exec(svg)?.[1] ?? "",
  width: /stroke-width="([^"]*)"/.exec(svg)?.[1] ?? "1.6",
});

const ICONS = {
  back,
  check,
  chevron,
  clock,
  close,
  cloud,
  column,
  deck,
  down,
  edit,
  eye,
  filter,
  grid,
  inbox,
  list,
  menu,
  more,
  order,
  play,
  plus,
  search,
  settings,
  shuffle,
  sort,
  subdeck,
  tag,
  trash,
  up,
} as const;

export type IconName = keyof typeof ICONS;

export function Icon(props: { name: IconName }) {
  const icon = parse(ICONS[props.name]);
  return (
    <svg class="icon" viewBox="0 0 20 20" width="20" height="20" aria-hidden="true">
      <path
        d={icon.d}
        fill="none"
        stroke="currentColor"
        stroke-width={icon.width}
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </svg>
  );
}
