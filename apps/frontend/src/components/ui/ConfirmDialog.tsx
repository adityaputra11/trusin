// Reusable themed confirmation dialog. Replaces the bare global `confirm()`
// (which renders an un-styled native dialog that doesn't match the dark theme
// and breaks in embedded webviews). Built on top of <Modal>, so it inherits
// the Escape-to-close, click-outside, and (once added) the a11y/focus-trap
// behavior.

import { Button } from "./Button";
import { Modal } from "./Modal";

interface ConfirmDialogProps {
  open: boolean;
  onClose: () => void;
  /** Discouraged body line, e.g. "This cannot be undone." */
  description?: string;
  /** Headline, e.g. "Delete event". */
  title: string;
  /** Confirm-button label. Defaults to "Confirm". */
  confirmLabel?: string;
  /** Cancel-button label. Defaults to "Cancel". */
  cancelLabel?: string;
  /** Renders the confirm button in the danger style + sets `aria-describedby`. */
  danger?: boolean;
  /** Disables the confirm button + shows a spinner. Pass the mutation's
   * `isPending` so the dialog reflects in-flight state. */
  loading?: boolean;
  onConfirm: () => void;
}

export function ConfirmDialog({
  open,
  onClose,
  title,
  description,
  confirmLabel = "Confirm",
  cancelLabel = "Cancel",
  danger = false,
  loading = false,
  onConfirm,
}: ConfirmDialogProps) {
  return (
    <Modal
      open={open}
      onClose={onClose}
      title={title}
      description={description}
      footer={
        <>
          <Button variant="ghost" size="sm" onClick={onClose} disabled={loading}>
            {cancelLabel}
          </Button>
          <Button
            variant={danger ? "danger" : "primary"}
            size="sm"
            loading={loading}
            onClick={onConfirm}
          >
            {confirmLabel}
          </Button>
        </>
      }
    >
      {/* Modal already renders `description`; this is intentionally empty so
       * the body slot exists for future richer confirm content. */}
      {null}
    </Modal>
  );
}
