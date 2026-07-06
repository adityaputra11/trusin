import { type ReactNode, useEffect, useId, useRef } from "react";
import { X } from "lucide-react";

interface ModalProps {
  open: boolean;
  onClose: () => void;
  title: string;
  description?: string;
  children: ReactNode;
  footer?: ReactNode;
}

export function Modal({ open, onClose, title, description, children, footer }: ModalProps) {
  const panelRef = useRef<HTMLDivElement>(null);
  // Remember the element that had focus before the dialog opened so we can
  // restore it on close (the trigger button).
  const previouslyFocused = useRef<HTMLElement | null>(null);
  const titleId = useId();
  const descId = useId();

  // Escape to close (existing behavior, kept here).
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  // Scroll lock + focus management while the dialog is open.
  useEffect(() => {
    if (!open) return;
    previouslyFocused.current = document.activeElement as HTMLElement | null;

    // Lock background scroll.
    const prevOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";

    // Move focus into the dialog (first focusable element, else the panel).
    const panel = panelRef.current;
    const focusFirst = () => {
      const target =
        panel?.querySelector<HTMLElement>(
          "button, [href], input, select, textarea, [tabindex]:not([tabindex='-1'])",
        ) ?? panel;
      target?.focus();
    };
    // Defer one tick so the panel is painted.
    const id = window.requestAnimationFrame(focusFirst);

    return () => {
      window.cancelAnimationFrame(id);
      document.body.style.overflow = prevOverflow;
      // Restore focus to the trigger.
      previouslyFocused.current?.focus();
    };
  }, [open]);

  // Focus trap: Tab/Shift+Tab cycle within the panel.
  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key !== "Tab") return;
    const panel = panelRef.current;
    if (!panel) return;
    const focusables = Array.from(
      panel.querySelectorAll<HTMLElement>(
        "button, [href], input, select, textarea, [tabindex]:not([tabindex='-1'])",
      ),
    ).filter((el) => !el.hasAttribute("disabled"));
    if (focusables.length === 0) return;
    const first = focusables[0];
    const last = focusables[focusables.length - 1];
    if (e.shiftKey && document.activeElement === first) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && document.activeElement === last) {
      e.preventDefault();
      first.focus();
    }
  };

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
      <div
        className="absolute inset-0 bg-black/60 backdrop-blur-sm"
        onClick={onClose}
      />
      <div
        ref={panelRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={description ? descId : undefined}
        tabIndex={-1}
        onKeyDown={onKeyDown}
        className="relative w-full max-w-md bg-card border border-border rounded-lg shadow-2xl outline-none"
      >
        <div className="flex items-start justify-between p-6 pb-4">
          <div>
            <h2 id={titleId} className="text-lg font-semibold text-foreground">
              {title}
            </h2>
            {description && (
              <p id={descId} className="text-xs text-muted mt-1">
                {description}
              </p>
            )}
          </div>
          <button
            onClick={onClose}
            className="text-muted hover:text-foreground transition-base rounded-md p-1"
            aria-label="Close"
          >
            <X className="h-5 w-5" />
          </button>
        </div>
        <div className="px-6 pb-4">{children}</div>
        {footer && (
          <div className="flex items-center justify-end gap-2 p-6 pt-4 border-t border-border">
            {footer}
          </div>
        )}
      </div>
    </div>
  );
}
