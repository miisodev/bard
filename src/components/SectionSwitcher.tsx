import { Inbox, Plus } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { api } from "../store";
import type { AppState } from "../types";

export default function SectionSwitcher({
  state,
  onClose,
  onPicked,
}: {
  state: AppState;
  onClose: () => void;
  onPicked: (id: number | null) => void;
}) {
  const [q, setQ] = useState("");
  const [highlight, setHighlight] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  const rows: { label: string; value: number | null; create?: boolean }[] = [];
  const qTrim = q.trim();
  rows.push({ label: "Inbox", value: null });
  for (const s of state.sections) {
    if (!qTrim || s.name.toLowerCase().includes(qTrim.toLowerCase())) {
      rows.push({ label: s.name, value: s.id });
    }
  }
  if (qTrim && !state.sections.some((s) => s.name.toLowerCase() === qTrim.toLowerCase())) {
    rows.push({ label: qTrim, value: null, create: true });
  }

  useEffect(() => inputRef.current?.focus(), []);

  const pick = (row: { label: string; value: number | null; create?: boolean }) => {
    if (row.create) {
      void api.createSection(row.label).then(onClose);
    } else {
      onPicked(row.value);
    }
  };

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setHighlight((h) => Math.min(rows.length - 1, h + 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setHighlight((h) => Math.max(0, h - 1));
    } else if (e.key === "Enter") {
      e.preventDefault();
      const row = rows[Math.min(highlight, rows.length - 1)];
      if (row) pick(row);
    } else if (e.key === "Escape") {
      onClose();
    }
  };

  return (
    <div className="sheet" onClick={onClose}>
      <div className="sheet-card compact" onClick={(e) => e.stopPropagation()}>
        <input
          ref={inputRef}
          className="field switcher-input"
          placeholder="Sections…"
          value={q}
          onChange={(e) => {
            setQ(e.target.value);
            setHighlight(1);
          }}
          onKeyDown={onKey}
        />
        <div className="switcher-list">
          {rows.map((row, i) => (
            <button
              key={row.create ? `c-${row.label}` : `${row.label}-${row.value}`}
              className={`switcher-row${i === highlight ? " on" : ""}`}
              onMouseEnter={() => setHighlight(i)}
              onClick={() => pick(row)}
            >
              {row.create ? <Plus size={13} strokeWidth={1.8} /> : <Inbox size={13} strokeWidth={1.8} />}
              <span>{row.create ? `Create section "${row.label}"` : row.label}</span>
            </button>
          ))}
        </div>
        <div className="sub" style={{ fontSize: 11, color: "var(--text-3)" }}>
          Capture and notes go to the active section · # Name creates one inline
        </div>
      </div>
    </div>
  );
}