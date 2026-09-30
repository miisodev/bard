import type { MouseEvent as ReactMouseEvent } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

type ResizeDir =
  | "East"
  | "North"
  | "NorthEast"
  | "NorthWest"
  | "South"
  | "SouthEast"
  | "SouthWest"
  | "West";

const EDGES: [string, ResizeDir][] = [
  ["t", "North"],
  ["b", "South"],
  ["l", "West"],
  ["r", "East"],
  ["lt", "NorthWest"],
  ["rt", "NorthEast"],
  ["lb", "SouthWest"],
  ["rb", "SouthEast"],
];

/**
 * Invisible edges and corners that resize a frameless window. Bard's windows
 * have no native frame, so without these a window can be moved but not sized.
 */
export default function ResizeHandles() {
  const start = (dir: ResizeDir) => (event: ReactMouseEvent) => {
    event.preventDefault();
    event.stopPropagation();
    void getCurrentWindow().startResizeDragging(dir);
  };
  return (
    <div className="resize-layer" aria-hidden="true">
      {EDGES.map(([edge, dir]) => (
        <div key={edge} className={`resize-handle h-${edge}`} onMouseDown={start(dir)} />
      ))}
    </div>
  );
}
