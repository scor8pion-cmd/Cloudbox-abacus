import type { ReactNode } from "react";
import { Cloud, FolderOpen, LayoutDashboard, RefreshCw, Settings as SettingsIcon } from "lucide-react";
import { cn } from "../lib/utils";
import { useConnectionStore } from "../stores/connectionStore";
import { useSettingsStore } from "../stores/settingsStore";

export type Page = "dashboard" | "files" | "sync" | "settings";

const NAV: { id: Page; label: string; icon: typeof Cloud }[] = [
  { id: "dashboard", label: "Dashboard", icon: LayoutDashboard },
  { id: "files", label: "Файлы", icon: FolderOpen },
  { id: "sync", label: "Синхронизация", icon: RefreshCw },
  { id: "settings", label: "Настройки", icon: SettingsIcon },
];

const STATUS_TEXT = { connected: "Подключено", connecting: "Подключение…", disconnected: "Отключено" } as const;

export function Layout({ page, onNavigate, children }: { page: Page; onNavigate: (p: Page) => void; children: ReactNode }) {
  const { status, latency_ms } = useConnectionStore();
  const host = useSettingsStore((s) => s.config.server.host);

  return (
    <div className="flex h-full">
      <aside className="flex w-60 shrink-0 flex-col border-r border-border/70 bg-slate-950/40 backdrop-blur-xl">
        <div className="flex items-center gap-2.5 px-5 py-5">
          <div className="flex h-9 w-9 items-center justify-center rounded-xl bg-gradient-to-br from-blue-500 to-blue-700 shadow-lg shadow-blue-900/40">
            <Cloud className="h-5 w-5 text-white" />
          </div>
          <div>
            <div className="text-base font-bold leading-tight">CloudBox</div>
            <div className="text-[11px] text-fg-muted">Hetzner Storage Box</div>
          </div>
        </div>

        <nav className="flex-1 space-y-1 px-3">
          {NAV.map(({ id, label, icon: Icon }) => (
            <button
              key={id}
              onClick={() => onNavigate(id)}
              className={cn(
                "flex w-full items-center gap-3 rounded-lg px-3 py-2.5 text-sm font-medium transition-colors",
                page === id ? "bg-accent/15 text-blue-300 ring-1 ring-accent/30" : "text-fg-muted hover:bg-slate-800/60 hover:text-fg",
              )}
            >
              <Icon className="h-4 w-4" />
              {label}
            </button>
          ))}
        </nav>

        <div className="m-3 rounded-lg border border-border/70 bg-slate-900/50 p-3">
          <div className="flex items-center gap-2 text-sm">
            <span
              className={cn(
                "h-2.5 w-2.5 rounded-full",
                status === "connected" && "bg-success shadow-[0_0_8px] shadow-success",
                status === "connecting" && "animate-pulse bg-warning",
                status === "disconnected" && "bg-error",
              )}
            />
            <span className="font-medium">{STATUS_TEXT[status]}</span>
          </div>
          <div className="mt-1 truncate text-[11px] text-fg-muted" title={host}>
            {host}
          </div>
          {status === "connected" && <div className="mt-0.5 text-[11px] text-fg-muted">Задержка: {latency_ms} мс</div>}
        </div>
      </aside>

      <main className="flex-1 overflow-y-auto p-6">{children}</main>
    </div>
  );
}
