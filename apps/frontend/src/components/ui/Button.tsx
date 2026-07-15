import { type ButtonHTMLAttributes, forwardRef } from "react";
import { Loader2 } from "lucide-react";

export type ButtonVariant =
  | "primary"
  | "ghost"
  | "success"
  | "danger"
  | "outline";
export type ButtonSize = "sm" | "md" | "lg";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  loading?: boolean;
}

const variants: Record<ButtonVariant, string> = {
  primary:
    "bg-[#19201b] text-white border border-border-light hover:bg-[#202a23] hover:border-border-hover shadow-[inset_0_1px_rgba(255,255,255,.025)]",
  ghost:
    "bg-transparent text-secondary border border-transparent hover:border-border hover:bg-hover hover:text-foreground",
  success:
    "bg-success text-[#041008] border border-success hover:bg-[#70e99b] shadow-[0_0_24px_rgba(74,222,128,.08)]",
  danger:
    "bg-danger text-white border border-danger hover:bg-[#dc2d2d]",
  outline:
    "bg-[rgba(10,13,11,.55)] text-foreground border border-border hover:bg-hover hover:border-border-hover",
};

const sizes: Record<ButtonSize, string> = {
  sm: "h-8 px-3 text-xs gap-1.5",
  md: "h-10 px-4 text-sm gap-2",
  lg: "h-11 px-5 text-sm gap-2",
};

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(
  (
    {
      variant = "primary",
      size = "md",
      loading = false,
      disabled,
      className = "",
      children,
      ...props
    },
    ref,
  ) => {
    return (
      <button
        ref={ref}
        disabled={disabled || loading}
        className={`inline-flex items-center justify-center rounded-md font-semibold transition-base disabled:opacity-50 disabled:pointer-events-none focus-visible:outline-2 focus-visible:outline-success focus-visible:outline-offset-2 ${variants[variant]} ${sizes[size]} ${className}`}
        {...props}
      >
        {loading && <Loader2 className="h-4 w-4 animate-spin" />}
        {children}
      </button>
    );
  },
);
Button.displayName = "Button";
