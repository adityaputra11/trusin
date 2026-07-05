interface PaginationProps {
  page: number;
  pages: number;
  total: number;
  onPageChange: (page: number) => void;
}

function pageList(page: number, pages: number): (number | "...")[] {
  if (pages <= 7) return Array.from({ length: pages }, (_, i) => i + 1);
  const out: (number | "...")[] = [1];
  const start = Math.max(2, page - 1);
  const end = Math.min(pages - 1, page + 1);
  if (start > 2) out.push("...");
  for (let i = start; i <= end; i++) out.push(i);
  if (end < pages - 1) out.push("...");
  out.push(pages);
  return out;
}

export function Pagination({ page, pages, total, onPageChange }: PaginationProps) {
  if (total === 0) return null;
  const items = pageList(page, pages);
  return (
    <div className="flex items-center justify-between px-3 py-3 text-sm text-muted">
      <span>{total.toLocaleString()} events</span>
      <div className="flex gap-1">
        {items.map((it, idx) =>
          it === "..." ? (
            <span key={`gap-${idx}`} className="px-2 py-1">
              …
            </span>
          ) : (
            <button
              key={it}
              onClick={() => onPageChange(it)}
              className={`px-2.5 py-1 rounded-md transition-base ${
                it === page
                  ? "bg-hover text-foreground"
                  : "text-secondary hover:bg-hover hover:text-foreground"
              }`}
            >
              {it}
            </button>
          ),
        )}
      </div>
    </div>
  );
}
