import { type HTMLAttributes, forwardRef } from "react";

interface CardProps extends HTMLAttributes<HTMLDivElement> {
  hover?: boolean;
}

export const Card = forwardRef<HTMLDivElement, CardProps>(
  ({ hover = false, className = "", children, ...props }, ref) => {
    return (
      <div
        ref={ref}
        className={`enterprise-panel bg-card border border-border rounded-lg p-6 ${
          hover ? "card-hover cursor-pointer" : ""
        } ${className}`}
        {...props}
      >
        {children}
      </div>
    );
  },
);
Card.displayName = "Card";

interface CardHeaderProps extends HTMLAttributes<HTMLDivElement> {
  title: string;
  subtitle?: string;
  action?: React.ReactNode;
}

export function CardHeader({
  title,
  subtitle,
  action,
  className = "",
}: CardHeaderProps) {
  return (
    <div className={`flex items-start justify-between mb-4 ${className}`}>
      <div>
        <h3 className="text-base font-semibold tracking-[-.015em] text-foreground leading-tight">
          {title}
        </h3>
        {subtitle && (
          <p className="text-xs text-muted mt-1">{subtitle}</p>
        )}
      </div>
      {action}
    </div>
  );
}
