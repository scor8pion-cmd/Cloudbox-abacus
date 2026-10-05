import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { ArrowRight, Clock, FolderOpen, Loader2, Pause, Pencil, Play, Plus, RefreshCw, Trash2, X } from "lucide-react";
import { Button } from "./ui/button";
import { Card, CardContent } from "./ui/card";
import { Badge } from "./ui/badge";
import { Progress } from "./ui/progress";
import { Input, Label } from "./ui/input";
import { api } from "../lib/api";
import type { ProfileStatus, SyncProfile, SyncSchedule } from "../lib/types";
import { errorText, formatBytes, formatIso, formatSpeed, newId } from "../lib/utils";
import { useSyncStore } from "../stores/syncStore";
import { useSettingsStore } from "../stores/settingsStore";
import { useConnectionStore } from "../stores/connectionStore";
import { refreshAll } from "../hooks/useTauriEvents";

const SCHEDULE_LABEL: Record<SyncSchedule, string> = { manual: "Вручную", realtime: "В реальном времени", interval: "По расписанию" };

function StatusBadge({ s, active }: { s?: ProfileStatus; active: boolean }) {
  if (active) return <Badge><Loader2 className="h-3 w-3 animate-spin" />Синхронизация</Badge>;
  switch (s?.status) {
    case "completed": return <Badge variant="success">Синхронизировано</Badge>;
    case "error": return <Badge variant="error">Ошибка</Badge>;
    case "cancelled": return <Badge variant="warning">Остановлено</Badge>;
    default: return <Badge variant="muted">Ожидание</Badge>;
  }
}

const blankProfile = (): SyncProfile => ({
  id: newId(),
  name: "Новый профиль",
  local_path: "",
  remote_path: "/backup",
  mode: "one-way-up",
  schedule: "manual",
  interval_minutes: 60,
  enabled: true,
  exclude: [".DS_Store", "Thumbs.db", "*.tmp", "~$*"],
});

function ProfileModal({ initial, onClose, onSave }: { initial: SyncProfile; onClose: () => void; onSave: (p: SyncProfile) => Promise<void> }) {
  const [p, setP] = useState<SyncProfile>(initial);
  const [excludeText, setExcludeText] = useState(initial.exclude.join(", "));
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const set = <K extends keyof SyncProfile>(k: K, v: SyncProfile[K]) => setP((x) => ({ ...x, [k]: v }));

  const pickFolder = async () => {
    const dir = await open({ directory: true, title: "Локальная папка" });
    if (typeof dir === "string") set("local_path", dir);
  };

  const submit = async () => {
    if (!p.name.trim() || !p.local_path.trim() || !p.remote_path.trim()) return setError("Заполните имя, локальную и удалённую папку");
    setSaving(true);
    try {
      await onSave({ ...p, exclude: excludeText.split(",").map((s) => s.trim()).filter(Boolean) });
      onClose();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm" onClick={onClose}>
      <Card className="w-[520px] bg-slate-900/95" onClick={(e) => e.stopPropagation()}>
        <div className="flex items-center justify-between border-b border-border px-5 py-4">
          <h2 className="text-lg font-semibold">{initial.local_path ? "Редактировать профиль" : "Новый профиль"}</h2>
          <button onClick={onClose} className="text-fg-muted hover:text-fg"><X className="h-5 w-5" /></button>
        </div>
        <div className="space-y-4 p-5">
          <div><Label>Название</Label><Input value={p.name} onChange={(e) => set("name", e.target.value)} /></div>
          <div>
            <Label>Локальная папка</Label>
            <div className="flex gap-2">
              <Input value={p.local_path} onChange={(e) => set("local_path", e.target.value)} placeholder="C:\Users\...\Documents" />
              <Button variant="secondary" onClick={pickFolder}><FolderOpen className="h-4 w-4" />Обзор</Button>
            </div>
          </div>
          <div><Label>Удалённая папка на Storage Box</Label><Input value={p.remote_path} onChange={(e) => set("remote_path", e.target.value)} placeholder="/backup/documents" /></div>
          <div className="grid grid-cols-2 gap-3">
            <div>
              <Label>Режим</Label>
              <select className="select" value={p.mode} onChange={(e) => set("mode", e.target.value as SyncProfile["mode"])}>
                <option value="one-way-up">Односторонний (ПК → облако)</option>
              </select>
            </div>
            <div>
              <Label>Расписание</Label>
              <select className="select" value={p.schedule} onChange={(e) => set("schedule", e.target.value as SyncSchedule)}>
                {Object.entries(SCHEDULE_LABEL).map(([k, v]) => <option key={k} value={k}>{v}</option>)}
              </select>
            </div>
          </div>
          {p.schedule === "interval" && (
            <div><Label>Интервал (минуты)</Label><Input type="number" min={1} value={p.interval_minutes} onChange={(e) => set("interval_minutes", Math.max(1, Number(e.target.value)))} /></div>
          )}
          <div><Label>Исключения (через запятую)</Label><Input value={excludeText} onChange={(e) => setExcludeText(e.target.value)} placeholder="*.tmp, node_modules" /></div>
          <label className="flex items-center gap-2 text-sm">
            <input type="checkbox" checked={p.enabled} onChange={(e) => set("enabled", e.target.checked)} className="h-4 w-4 accent-blue-600" />
            Профиль активен
          </label>
          {error && <div className="rounded-lg bg-error/10 p-2 text-sm text-red-300">{error}</div>}
        </div>
        <div className="flex justify-end gap-2 border-t border-border px-5 py-4">
          <Button variant="ghost" onClick={onClose}>Отмена</Button>
          <Button onClick={submit} disabled={saving}>{saving && <Loader2 className="h-4 w-4 animate-spin" />}Сохранить</Button>
        </div>
      </Card>
    </div>
  );
}

export function SyncProfiles() {
  const { profiles, statuses, progress, activeSync } = useSyncStore();
  const { config, save } = useSettingsStore();
  const connected = useConnectionStore((s) => s.status === "connected");
  const [editing, setEditing] = useState<SyncProfile | null>(null);
  const [error, setError] = useState<string | null>(null);

  const saveProfile = async (p: SyncProfile) => {
    const exists = config.profiles.some((x) => x.id === p.id);
    const list = exists ? config.profiles.map((x) => (x.id === p.id ? p : x)) : [...config.profiles, p];
    await save({ ...config, profiles: list });
  };

  const remove = async (p: SyncProfile) => {
    if (!window.confirm(`Удалить профиль «${p.name}»? Файлы на сервере не будут удалены.`)) return;
    await save({ ...config, profiles: config.profiles.filter((x) => x.id !== p.id) }).catch((e) => setError(errorText(e)));
  };

  const act = async (fn: () => Promise<void>) => {
    setError(null);
    try { await fn(); await refreshAll(); } catch (e) { setError(errorText(e)); }
  };

  return (
    <div className="space-y-5">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold">Синхронизация</h1>
          <p className="text-sm text-fg-muted">Профили односторонней синхронизации: локальная папка → Storage Box</p>
        </div>
        <Button onClick={() => setEditing(blankProfile())}><Plus className="h-4 w-4" />Новый профиль</Button>
      </div>

      {error && <div className="rounded-lg border border-error/40 bg-error/10 p-3 text-sm text-red-200">{error}</div>}
      {!connected && <div className="rounded-lg border border-warning/40 bg-warning/10 p-3 text-sm text-amber-200">Нет подключения — запуск синхронизации недоступен.</div>}

      {profiles.length === 0 ? (
        <Card><CardContent className="flex flex-col items-center py-16 text-fg-muted">
          <RefreshCw className="mb-3 h-10 w-10" />
          <div>Профилей пока нет</div>
          <Button className="mt-4" onClick={() => setEditing(blankProfile())}><Plus className="h-4 w-4" />Создать первый профиль</Button>
        </CardContent></Card>
      ) : (
        <div className="grid grid-cols-2 gap-4">
          {profiles.map((p) => {
            const s = statuses[p.id];
            const active = activeSync.has(p.id);
            const live = progress[p.id];
            const pct = active ? (live?.progress ?? s?.progress ?? 0) * 100 : s?.status === "completed" ? 100 : 0;
            return (
              <Card key={p.id} className={!p.enabled ? "opacity-60" : ""}>
                <CardContent className="space-y-3 pt-5">
                  <div className="flex items-start justify-between gap-2">
                    <div className="min-w-0">
                      <div className="text-ellipsis overflow-hidden whitespace-nowrap text-base font-semibold">{p.name}</div>
                      <div className="mt-1 flex items-center gap-1.5 text-xs text-fg-muted">
                        <span className="overflow-hidden text-ellipsis whitespace-nowrap" title={p.local_path}>{p.local_path || "—"}</span>
                        <ArrowRight className="h-3 w-3 shrink-0 text-accent" />
                        <span className="overflow-hidden text-ellipsis whitespace-nowrap" title={p.remote_path}>{p.remote_path}</span>
                      </div>
                    </div>
                    <StatusBadge s={s} active={active} />
                  </div>
                  <div className="flex flex-wrap gap-1.5">
                    <Badge variant="muted">ПК → облако</Badge>
                    <Badge variant="muted"><Clock className="h-3 w-3" />{SCHEDULE_LABEL[p.schedule]}{p.schedule === "interval" ? ` · ${p.interval_minutes} мин` : ""}</Badge>
                  </div>
                  <div>
                    <Progress value={pct} />
                    <div className="mt-1.5 flex justify-between text-xs text-fg-muted">
                      {active && live ? (
                        <>
                          <span className="overflow-hidden text-ellipsis whitespace-nowrap pr-2">{live.file || "Сканирование…"}</span>
                          <span className="whitespace-nowrap">{formatBytes(live.uploaded)} / {formatBytes(live.total)} · {formatSpeed(live.speed)}</span>
                        </>
                      ) : (
                        <>
                          <span>Последняя: {formatIso(s?.last_sync)}</span>
                          {s?.files_synced ? <span>{s.files_synced} файл(ов), {formatBytes(s.bytes_transferred)}</span> : null}
                        </>
                      )}
                    </div>
                    {s?.last_error && !active && <div className="mt-1 overflow-hidden text-ellipsis whitespace-nowrap text-xs text-red-300" title={s.last_error}>{s.last_error}</div>}
                  </div>
                  <div className="flex gap-2 pt-1">
                    {active ? (
                      <Button size="sm" variant="secondary" onClick={() => act(() => api.stopSync(p.id))}><Pause className="h-3.5 w-3.5" />Пауза</Button>
                    ) : (
                      <Button size="sm" disabled={!connected || !p.enabled} onClick={() => act(() => api.startSync(p.id))}><Play className="h-3.5 w-3.5" />Запустить</Button>
                    )}
                    <Button size="sm" variant="ghost" onClick={() => setEditing(p)}><Pencil className="h-3.5 w-3.5" />Редактировать</Button>
                    <Button size="sm" variant="ghost" className="ml-auto text-red-300 hover:text-red-200" disabled={active} onClick={() => remove(p)}><Trash2 className="h-3.5 w-3.5" />Удалить</Button>
                  </div>
                </CardContent>
              </Card>
            );
          })}
        </div>
      )}

      {editing && <ProfileModal initial={editing} onClose={() => setEditing(null)} onSave={saveProfile} />}
    </div>
  );
}
