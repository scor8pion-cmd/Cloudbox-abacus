import type { HTMLAttributes } from "react";
import { cn } from "../../lib/utils";

type Variant = "default" | "success" | "warning" | "error" | "muted";

const variants: Record<Variant, string> = {
  default: "bg-accent/15 text-blue-300 border-accent/30",
  success: "bg-success/15 text-emerald-300 border-success/30",
  warning: "bg-warning/15 text-amber-300 border-warning/30",
  error: "bg-error/15 text-red-300 border-error/30",
  muted: "bg-slate-700/40 text-fg-muted border-border",
};

export function Badge({ className, variant = "default", ...props }: HTMLAttributes<HTMLSpanElement> & { variant?: Variant }) {
  return (
    <span
      className={cn("inline-flex items-center gap-1 rounded-full border px-2.5 py-0.5 text-xs font-medium", variants[variant], className)}
      {...props}
    />
  );
}
