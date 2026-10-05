import { useState } from "react";
import { Activity, AlertTriangle, CheckCircle2, Clock, HardDrive, Loader2, Plug, RefreshCw, Server, XCircle } from "lucide-react";
import { Card, CardContent, CardHeader, CardTitle } from "./ui/card";
import { Button } from "./ui/button";
import { Badge } from "./ui/badge";
import { Progress } from "./ui/progress";
import { SpeedChart } from "./SpeedChart";
import type { Page } from "./Layout";
import { api } from "../lib/api";
import { errorText, formatBytes, formatIso, formatSpeed } from "../lib/utils";
import { useConnectionStore } from "../stores/connectionStore";
import { useSyncStore } from "../stores/syncStore";
import { useSettingsStore } from "../stores/settingsStore";
import { refreshAll } from "../hooks/useTauriEvents";

const OP_LABEL: Record<string, string> = { upload: "Загрузка", download: "Скачивание", delete: "Удаление", mkdir: "Папка" };

function OpStatus({ status }: { status: string }) {
  if (status === "done") return <Badge variant="success"><CheckCircle2 className="h-3 w-3" />Готово</Badge>;
  if (status === "error") return <Badge variant="error"><XCircle className="h-3 w-3" />Ошибка</Badge>;
  if (status === "in_progress") return <Badge><Loader2 className="h-3 w-3 animate-spin" />В процессе</Badge>;
  return <Badge variant="muted"><Clock className="h-3 w-3" />В очереди</Badge>;
}

export function Dashboard({ onNavigate }: { onNavigate: (p: Page) => void }) {
  const { status, latency_ms, storage, mount, upload_speed, download_speed } = useConnectionStore();
  const { operations, profiles, activeSync, lastError, setError } = useSyncStore();
  const config = useSettingsStore((s) => s.config);
  const [busy, setBusy] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  const usedPct = storage.total ? (storage.used / storage.total) * 100 : 0;
  const connected = status === "connected";

  const run = async (key: string, fn: () => Promise<unknown>) => {
    setBusy(key);
    setMessage(null);
    try {
      await fn();
      await refreshAll();
    } catch (e) {
      setMessage(errorText(e));
    } finally {
      setBusy(null);
    }
  };

  const syncAll = () =>
    run("sync", async () => {
      const enabled = profiles.filter((p) => p.enabled);
      if (!enabled.length) throw "Нет активных профилей синхронизации. Создайте профиль в разделе «Синхронизация».";
      await Promise.all(enabled.map((p) => api.startSync(p.id)));
    });

  return (
    <div className="space-y-5">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold">Обзор</h1>
          <p className="text-sm text-fg-muted">{config.server.username}@{config.server.host}</p>
        </div>
        <div className="flex gap-2">
          {!connected && (
            <Button variant="secondary" disabled={busy !== null || status === "connecting"} onClick={() => run("connect", () => api.connect(config, ""))}>
              {busy === "connect" ? <Loader2 className="h-4 w-4 animate-spin" /> : <Plug className="h-4 w-4" />}Подключиться
            </Button>
          )}
          {mount.status === "mounted" ? (
            <Button variant="outline" disabled={busy !== null} onClick={() => run("mount", api.unmountDrive)}>
              <HardDrive className="h-4 w-4" />Отмонтировать
            </Button>
          ) : (
            <Button variant="outline" disabled={busy !== null || !mount.rclone_available} onClick={() => run("mount", api.mountDrive)}
              title={mount.rclone_available ? "" : "rclone не найден"}>
              {busy === "mount" ? <Loader2 className="h-4 w-4 animate-spin" /> : <HardDrive className="h-4 w-4" />}Монтировать диск
            </Button>
          )}
          <Button disabled={busy !== null || !connected} onClick={syncAll}>
            <RefreshCw className={activeSync.size ? "h-4 w-4 animate-spin" : "h-4 w-4"} />Синхр. сейчас
          </Button>
        </div>
      </div>

      {(message || lastError) && (
        <div className="flex items-start gap-2 rounded-lg border border-error/40 bg-error/10 p-3 text-sm text-red-200">
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
          <span className="flex-1 whitespace-pre-wrap break-all">{message ?? lastError}</span>
          <button className="text-xs text-red-300 hover:underline" onClick={() => { setMessage(null); setError(null); }}>Скрыть</button>
        </div>
      )}

      <div className="grid grid-cols-3 gap-4">
        <Card>
          <CardHeader><CardTitle><Server className="h-4 w-4" />Соединение</CardTitle></CardHeader>
          <CardContent>
            <div className="text-xl font-semibold">
              {connected ? <span className="text-success">Подключено</span> : status === "connecting" ? <span className="text-warning">Подключение…</span> : <span className="text-error">Отключено</span>}
            </div>
            <div className="mt-2 space-y-1 text-xs text-fg-muted">
              <div>SFTP · порт {config.server.ssh_port}</div>
              <div>Задержка: {connected ? `${latency_ms} мс` : "—"}</div>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader><CardTitle><HardDrive className="h-4 w-4" />Сетевой диск</CardTitle></CardHeader>
          <CardContent>
            <div className="text-xl font-semibold">
              {mount.status === "mounted" ? <span className="text-success">Смонтирован</span> : mount.status === "error" ? <span className="text-error">Ошибка</span> : <span className="text-fg-muted">Не смонтирован</span>}
            </div>
            <div className="mt-2 space-y-1 text-xs text-fg-muted">
              <div>Точка: {mount.mount_point ?? config.mount.mount_point ?? "—"}</div>
              <div>{mount.rclone_available ? `Кэш: ${config.mount.cache_mode}, до ${config.mount.cache_max_size}` : "rclone не установлен"}</div>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader><CardTitle><Activity className="h-4 w-4" />Использование места</CardTitle></CardHeader>
          <CardContent>
            <div className="text-xl font-semibold">
              {formatBytes(storage.used)} <span className="text-sm font-normal text-fg-muted">из {storage.total ? formatBytes(storage.total) : "—"}</span>
            </div>
            <Progress value={usedPct} className="mt-3" indicatorClassName={usedPct > 90 ? "bg-error from-red-600 to-red-400" : undefined} />
            <div className="mt-2 text-xs text-fg-muted">{usedPct.toFixed(1)}% занято</div>
          </CardContent>
        </Card>
      </div>

      <Card>
        <CardHeader>
          <CardTitle><Activity className="h-4 w-4" />Скорость передачи (60 сек)</CardTitle>
          <div className="flex gap-4 text-xs">
            <span className="text-blue-400">↑ {formatSpeed(upload_speed)}</span>
            <span className="text-emerald-400">↓ {formatSpeed(download_speed)}</span>
          </div>
        </CardHeader>
        <CardContent><SpeedChart /></CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle><Clock className="h-4 w-4" />Последние операции</CardTitle>
          <Button variant="ghost" size="sm" onClick={() => onNavigate("sync")}>Профили →</Button>
        </CardHeader>
        <CardContent>
          {operations.length === 0 ? (
            <div className="py-8 text-center text-sm text-fg-muted">Операций пока нет</div>
          ) : (
            <div className="max-h-72 overflow-y-auto">
              <table className="w-full text-sm">
                <thead className="sticky top-0 bg-slate-800/95 text-left text-xs text-fg-muted">
                  <tr>
                    <th className="px-2 py-2 font-medium">Тип</th>
                    <th className="px-2 py-2 font-medium">Файл</th>
                    <th className="px-2 py-2 font-medium">Статус</th>
                    <th className="px-2 py-2 font-medium">Время</th>
                  </tr>
                </thead>
                <tbody>
                  {operations.slice(0, 50).map((op) => (
                    <tr key={op.id} className="border-t border-border/50" title={op.error_msg ?? ""}>
                      <td className="px-2 py-2 text-fg-muted">{OP_LABEL[op.op_type] ?? op.op_type}</td>
                      <td className="max-w-md truncate px-2 py-2">{op.remote_path || op.local_path}</td>
                      <td className="px-2 py-2"><OpStatus status={op.status} /></td>
                      <td className="whitespace-nowrap px-2 py-2 text-xs text-fg-muted">{formatIso(op.updated_at)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
