import { Loader2 } from "lucide-react";

export function Spinner({ size = 20 }: { size?: number }) {
  return <Loader2 className="animate-spin text-muted" style={{ width: size, height: size }} />;
}

export function FullSpinner({ label }: { label?: string }) {
  return (
    <div className="flex flex-col items-center justify-center gap-3 py-16 text-muted">
      <Spinner size={24} />
      {label && <p className="text-sm">{label}</p>}
    </div>
  );
}
