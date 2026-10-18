import {
  Droplet,
  Image as ImageIcon,
  Link as LinkIcon,
  Mic,
  PenTool,
  Type,
  type LucideIcon,
} from "lucide-react";
import type { ShelfKind } from "../shelf";

/*
  The kind's glyph, shared by every surface that names an item's kind: the
  All zone's cards, the pack card's preview tiles and the pack view's rows.
  Colour is never the only carrier, so the glyph always sits beside the written
  label or the item's own text.
*/

const GLYPHS: Record<ShelfKind, LucideIcon> = {
  text: Type,
  image: ImageIcon,
  link: LinkIcon,
  color: Droplet,
  dictation: Mic,
  draw: PenTool,
};

export default function KindGlyph({ kind, size = 13 }: { kind: ShelfKind; size?: number }) {
  const Glyph = GLYPHS[kind];
  return <Glyph size={size} strokeWidth={2} aria-hidden="true" />;
}
