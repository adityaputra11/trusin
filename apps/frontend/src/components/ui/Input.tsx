import { forwardRef, type InputHTMLAttributes, type SelectHTMLAttributes, type TextareaHTMLAttributes } from "react";

const baseField =
  "w-full bg-[rgba(7,10,8,.8)] border border-border rounded-md text-foreground placeholder:text-muted px-3.5 py-2.5 text-sm transition-base focus:outline-none focus:border-[rgba(74,222,128,.55)] focus:ring-2 focus:ring-[rgba(74,222,128,.06)]";

export const Input = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement>>(
  ({ className = "", ...props }, ref) => (
    <input ref={ref} className={`${baseField} ${className}`} {...props} />
  ),
);
Input.displayName = "Input";

export const Textarea = forwardRef<HTMLTextAreaElement, TextareaHTMLAttributes<HTMLTextAreaElement>>(
  ({ className = "", ...props }, ref) => (
    <textarea ref={ref} className={`${baseField} font-mono ${className}`} {...props} />
  ),
);
Textarea.displayName = "Textarea";

export const Select = forwardRef<HTMLSelectElement, SelectHTMLAttributes<HTMLSelectElement>>(
  ({ className = "", children, ...props }, ref) => (
    <select
      ref={ref}
      className={`${baseField} appearance-none cursor-pointer pr-9 bg-[length:18px] bg-[right_12px_center] bg-no-repeat ${className}`}
      style={{
        backgroundImage:
          "url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='18' height='18' viewBox='0 0 24 24' fill='none' stroke='%237A7A7A' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpolyline points='6 9 12 15 18 9'%3E%3C/polyline%3E%3C/svg%3E\")",
      }}
      {...props}
    >
      {children}
    </select>
  ),
);
Select.displayName = "Select";

interface FieldProps {
  label: string;
  hint?: string;
  htmlFor?: string;
  children: React.ReactNode;
  className?: string;
}

export function Field({ label, hint, htmlFor, children, className = "" }: FieldProps) {
  return (
    <div className={`flex flex-col gap-1.5 ${className}`}>
      <label htmlFor={htmlFor} className="text-xs font-medium text-secondary">
        {label}
      </label>
      {children}
      {hint && <p className="text-xs text-muted">{hint}</p>}
    </div>
  );
}
