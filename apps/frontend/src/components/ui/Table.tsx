import { type ReactNode, type ThHTMLAttributes, type TdHTMLAttributes, type TableHTMLAttributes } from "react";

interface TableProps extends TableHTMLAttributes<HTMLTableElement> {
  children: ReactNode;
}

export function Table({ children, className = "", ...props }: TableProps) {
  return (
    <div className="w-full overflow-x-auto">
      <table className={`w-full border-collapse ${className}`} {...props}>
        {children}
      </table>
    </div>
  );
}

export function THead({ children }: { children: ReactNode }) {
  return (
    <thead>
      <tr className="border-y border-border bg-[rgba(255,255,255,.012)]">{children}</tr>
    </thead>
  );
}

export function TH({
  children,
  className = "",
  ...props
}: ThHTMLAttributes<HTMLTableCellElement>) {
  return (
    <th
      className={`text-left text-[10px] font-semibold uppercase tracking-[.1em] text-muted py-3.5 px-4 ${className}`}
      {...props}
    >
      {children}
    </th>
  );
}

export function TBody({ children }: { children: ReactNode }) {
  return <tbody>{children}</tbody>;
}

export function TR({
  children,
  className = "",
  onClick,
}: {
  children: ReactNode;
  className?: string;
  onClick?: () => void;
}) {
  return (
    <tr
      onClick={onClick}
      className={`border-b border-border last:border-0 transition-base ${
        onClick ? "cursor-pointer hover:bg-[rgba(74,222,128,.025)]" : "hover:bg-card-secondary"
      } ${className}`}
    >
      {children}
    </tr>
  );
}

export function TD({
  children,
  className = "",
  ...props
}: TdHTMLAttributes<HTMLTableCellElement>) {
  return (
    <td className={`py-3.5 px-4 text-sm align-middle ${className}`} {...props}>
      {children}
    </td>
  );
}
