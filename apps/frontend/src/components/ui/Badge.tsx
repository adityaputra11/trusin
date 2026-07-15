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
  success: "bg-[rgba(74,222,128,.09)] text-success border border-[rgba(74,222,128,.18)]",
  warning: "bg-[rgba(245,158,11,.09)] text-warning border border-[rgba(245,158,11,.18)]",
  danger: "bg-[rgba(239,68,68,.09)] text-danger border border-[rgba(239,68,68,.18)]",
  info: "bg-[rgba(59,130,246,.09)] text-info border border-[rgba(59,130,246,.18)]",
  purple: "bg-[rgba(147,51,234,.09)] text-purple border border-[rgba(147,51,234,.18)]",
  neutral: "bg-[rgba(255,255,255,.035)] text-secondary border border-border",
};

export function Badge({
  variant = "neutral",
  className = "",
  children,
  ...props
}: BadgeProps) {
  return (
    <span
      className={`inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] tracking-wide font-semibold ${variants[variant]} ${className}`}
      {...props}
    >
      {children}
    </span>
  );
}
