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
    "bg-hover text-white border border-border-light hover:bg-[#242424]",
  ghost:
    "bg-transparent text-secondary border border-border-light hover:bg-hover hover:text-foreground",
  success:
    "bg-success text-white border border-success hover:bg-[#1ea753]",
  danger:
    "bg-danger text-white border border-danger hover:bg-[#dc2d2d]",
  outline:
    "bg-transparent text-foreground border border-border hover:bg-hover",
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
        className={`inline-flex items-center justify-center rounded-md font-medium transition-base disabled:opacity-50 disabled:pointer-events-none focus-visible:outline-2 focus-visible:outline-success focus-visible:outline-offset-2 ${variants[variant]} ${sizes[size]} ${className}`}
        {...props}
      >
        {loading && <Loader2 className="h-4 w-4 animate-spin" />}
        {children}
      </button>
    );
  },
);
Button.displayName = "Button";
