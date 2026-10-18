import { pageNumbers } from "../shelf";

/*
  A zone's pager: page numbers with the middle elided, the current page marked
  by shape and by `aria-current` — never by colour alone. It renders even on a
  single page so the strip it sits in never changes height and the cards above
  never shift.

  Each zone pages independently; the All zone is the only one with pages so
  far.
*/
export default function Pager({
  page,
  pages,
  onPage,
  label,
}: {
  /** Zero-based. */
  page: number;
  pages: number;
  onPage: (page: number) => void;
  label: string;
}) {
  return (
    <div className="pager" role="group" aria-label={label}>
      {pages > 1
        ? pageNumbers(page + 1, pages).map((entry, index) =>
            entry === "gap" ? (
              <span key={`gap-${index}`} className="pager-gap" aria-hidden="true">
                …
              </span>
            ) : (
              <button
                key={entry}
                type="button"
                className={`pager-page${entry === page + 1 ? " on" : ""}`}
                aria-current={entry === page + 1 ? "page" : undefined}
                aria-label={`Page ${entry} of ${pages}`}
                onClick={() => onPage(entry - 1)}
              >
                {entry}
              </button>
            ),
          )
        : null}
    </div>
  );
}
