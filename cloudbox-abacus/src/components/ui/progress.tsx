import { cn } from "../../lib/utils";

interface ProgressProps {
  value: number; // 0..100
  className?: string;
  indicatorClassName?: string;
}

export function Progress({ value, className, indicatorClassName }: ProgressProps) {
  const v = Math.max(0, Math.min(100, value || 0));
  return (
    <div className={cn("h-2 w-full overflow-hidden rounded-full bg-slate-700/60", className)} role="progressbar" aria-valuenow={v}>
      <div
        className={cn("h-full rounded-full bg-gradient-to-r from-blue-600 to-sky-400 transition-all duration-300", indicatorClassName)}
        style={{ width: `${v}%` }}
      />
    </div>
  );
}
