import { type HTMLAttributes } from "react";

export type BadgeVariant =
  | "success"
  | "warning"
  | "danger"
  | "info"
  | "purple"
  | "neutral";

interface BadgeProps extends HTMLAttributes<HTMLSpanElement> {
  variant?: BadgeVariant;
}

const variants: Record<BadgeVariant, string> = {
  success: "bg-[rgba(34,197,94,.15)] text-success",
  warning: "bg-[rgba(245,158,11,.15)] text-warning",
  danger: "bg-[rgba(239,68,68,.15)] text-danger",
  info: "bg-[rgba(59,130,246,.15)] text-info",
  purple: "bg-[rgba(147,51,234,.15)] text-purple",
  neutral: "bg-[rgba(255,255,255,.06)] text-secondary",
};

export function Badge({
  variant = "neutral",
  className = "",
  children,
  ...props
}: BadgeProps) {
  return (
    <span
      className={`inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium ${variants[variant]} ${className}`}
      {...props}
    >
      {children}
    </span>
  );
}
