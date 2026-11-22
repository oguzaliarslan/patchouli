// Small inline icon set (Material Symbols outlines, simplified).

const PATHS = {
  home: "M4 10.5 12 4l8 6.5V20a1 1 0 0 1-1 1h-5v-6h-4v6H5a1 1 0 0 1-1-1z",
  list: "M4 6h2v2H4zm4 0h12v2H8zM4 11h2v2H4zm4 0h12v2H8zm-4 5h2v2H4zm4 0h12v2H8z",
  search:
    "M10.5 4a6.5 6.5 0 0 1 5.2 10.4l4.4 4.4-1.4 1.4-4.4-4.4A6.5 6.5 0 1 1 10.5 4zm0 2a4.5 4.5 0 1 0 0 9 4.5 4.5 0 0 0 0-9z",
  history:
    "M13 4a8 8 0 1 1-7.6 10.5l1.9-.6A6 6 0 1 0 7.4 8H10v2H4V4h2v2.4A8 8 0 0 1 13 4zm-1 4h2v4.6l3 1.8-1 1.7-4-2.4z",
  folder: "M3 6a1 1 0 0 1 1-1h6l2 2h8a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1z",
  settings:
    "M12 8.5a3.5 3.5 0 1 1 0 7 3.5 3.5 0 0 1 0-7zm-1.4-6h2.8l.5 2.6 1.6.7 2.2-1.5 2 2-1.5 2.2.7 1.6 2.6.5v2.8l-2.6.5-.7 1.6 1.5 2.2-2 2-2.2-1.5-1.6.7-.5 2.6h-2.8l-.5-2.6-1.6-.7-2.2 1.5-2-2 1.5-2.2-.7-1.6-2.6-.5v-2.8l2.6-.5.7-1.6-1.5-2.2 2-2 2.2 1.5 1.6-.7z",
  sync: "M12 4a8 8 0 0 1 7.4 5H17v2h6V5h-2v2.3A10 10 0 0 0 2 12h2a8 8 0 0 1 8-8zm8 8a8 8 0 0 1-15.4 3H7v-2H1v6h2v-2.3A10 10 0 0 0 22 12z",
  grid: "M4 4h7v7H4zm9 0h7v7h-7zM4 13h7v7H4zm9 0h7v7h-7z",
  rows: "M4 5h16v3H4zm0 5.5h16v3H4zM4 16h16v3H4z",
  close: "m6.4 5 5.6 5.6L17.6 5 19 6.4 13.4 12l5.6 5.6-1.4 1.4-5.6-5.6L6.4 19 5 17.6l5.6-5.6L5 6.4z",
  plus: "M11 5h2v6h6v2h-6v6h-2v-6H5v-2h6z",
  minus: "M5 11h14v2H5z",
  external: "M14 4h6v6h-2V7.4l-7.3 7.3-1.4-1.4L16.6 6H14zM5 6h6v2H6v10h10v-5h2v6a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1z",
  play: "M8 5v14l11-7z",
  refresh:
    "M12 5a7 7 0 0 1 6.3 4H15v2h6V5h-2v2.1A9 9 0 1 0 21 12h-2a7 7 0 1 1-7-7z",
  download: "M11 4h2v8.2l3.3-3.3 1.4 1.4L12 16l-5.7-5.7 1.4-1.4 3.3 3.3zM5 18h14v2H5z",
  calendar:
    "M7 3h2v2h6V3h2v2h2a1 1 0 0 1 1 1v13a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1h2zM6 10v8h12v-8zm2 2h3v3H8z",
  clock:
    "M12 3a9 9 0 1 1 0 18 9 9 0 0 1 0-18zm0 2a7 7 0 1 0 0 14 7 7 0 0 0 0-14zm-1 2h2v4.6l3.2 3.2-1.4 1.4L11 12.4z",
  person:
    "M12 4a4 4 0 1 1 0 8 4 4 0 0 1 0-8zm0 2a2 2 0 1 0 0 4 2 2 0 0 0 0-4zm0 8c4 0 7 2 7 4.5V20H5v-1.5C5 16 8 14 12 14zm0 2c-2.6 0-4.4 1-4.9 2h9.8c-.5-1-2.3-2-4.9-2z",
} as const;

export type IconName = keyof typeof PATHS;

export function Icon({ name, size = 18 }: { name: IconName; size?: number }) {
  return (
    <svg
      className="icon"
      width={size}
      height={size}
      viewBox="0 0 24 24"
      aria-hidden="true"
      fill="currentColor"
    >
      <path d={PATHS[name]} />
    </svg>
  );
}
