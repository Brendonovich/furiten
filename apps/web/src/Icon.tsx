// Small line icons, drawn at 20px with the current text colour.
const PATHS = {
  plus: "M10 4v12M4 10h12",
  deck: "M4.5 15.5v-11A1.5 1.5 0 0 1 6 3h9.5v11H6a1.5 1.5 0 0 0 0 3h9.5M8 6.5h4.5",
  subdeck: "M6 5v6.5a2 2 0 0 0 2 2h7M12 10.5l3 3-3 3",
  tag: "M8.5 3.5l-2 13M13.5 3.5l-2 13M4.5 7.5h12M3.5 12.5h12",
  cloud: "M6 15.5h8.5a3.5 3.5 0 0 0 .4-7 5 5 0 0 0-9.6 1.4A2.8 2.8 0 0 0 6 15.5z",
  inbox: "M3.5 11l2.2-6h8.6l2.2 6v3.5a2 2 0 0 1-2 2h-9a2 2 0 0 1-2-2zM3.5 11h4l1 2h3l1-2h4",
  menu: "M3.5 6h13M3.5 10h13M3.5 14h13",
  search: "M9 15a6 6 0 1 0 0-12 6 6 0 0 0 0 12zM13.5 13.5L17 17",
  close: "M5 5l10 10M15 5L5 15",
  grid: "M4 4h5v5H4zM11 4h5v5h-5zM4 11h5v5H4zM11 11h5v5h-5z",
  list: "M4 4.5h12v4H4zM4 11.5h12v4H4z",
  column: "M4 4h4.5v12H4zM10.5 4H16v12h-5.5z",
  sort: "M6 4v12M3 13l3 3 3-3M14 16V4M11 7l3-3 3 3",
  order: "M4 6h12M6 10h8M8.5 14h3",
  up: "M10 16V4M5 9l5-5 5 5",
  down: "M10 4v12M5 11l5 5 5-5",
  chevron: "M6 8l4 4 4-4",
  shuffle: "M3 6h3l8 8h3M3 14h3l8-8h3M15 4l2 2-2 2M15 12l2 2-2 2",
  more: "M5 10h.01M10 10h.01M15 10h.01",
  edit: "M4 16l1-4 8-8 3 3-8 8zM11.5 5.5l3 3",
  check: "M4.5 10.5l3.5 3.5 7.5-8",
  back: "M12.5 4.5L7 10l5.5 5.5",
  trash: "M4.5 6h11M8 6V4.5h4V6M6 6l.7 9.5h6.6L14 6",
  play: "M7 4.5v11l9-5.5z",
  filter: "M3.5 4.5h13l-5 6v4.5l-3 1.5v-6z",
  eye: "M2.5 10s2.7-5.5 7.5-5.5 7.5 5.5 7.5 5.5-2.7 5.5-7.5 5.5S2.5 10 2.5 10zM10 12.2a2.2 2.2 0 1 0 0-4.4 2.2 2.2 0 0 0 0 4.4z",
} as const;

export type IconName = keyof typeof PATHS;

export function Icon(props: { name: IconName }) {
  return (
    <svg class="icon" viewBox="0 0 20 20" width="20" height="20" aria-hidden="true">
      <path
        d={PATHS[props.name]}
        fill="none"
        stroke="currentColor"
        stroke-width={props.name === "more" ? "2.6" : "1.6"}
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </svg>
  );
}
