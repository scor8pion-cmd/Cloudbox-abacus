import { useEffect, useState } from "react";
import { CheckCircle2, Eye, EyeOff, HardDrive, KeyRound, Loader2, Monitor, Plug, Save, Server, Unplug, XCircle } from "lucide-react";
import { Button } from "./ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "./ui/card";
import { Input, Label } from "./ui/input";
import { Badge } from "./ui/badge";
import { api } from "../lib/api";
import type { Config } from "../lib/types";
import { errorText } from "../lib/utils";
import { useSettingsStore } from "../stores/settingsStore";
import { useConnectionStore } from "../stores/connectionStore";

export function Settings() {
  const { config, save, passwordStored, setPasswordStored } = useSettingsStore();
  const status = useConnectionStore((s) => s.status);
  const [form, setForm] = useState<Config>(config);
  const [password, setPassword] = useState("");
  const [showPw, setShowPw] = useState(false);
  const [rememberPw, setRememberPw] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [test, setTest] = useState<{ ok: boolean; text: string } | null>(null);
  const [saved, setSaved] = useState<string | null>(null);

  useEffect(() => setForm(config), [config]);

  const upd = <S extends keyof Config>(section: S, patch: Partial<Config[S]>) =>
    setForm((f) => ({ ...f, [section]: { ...(f[section] as object), ...patch } }));

  const withBusy = async (key: string, fn: () => Promise<void>) => {
    setBusy(key);
    setSaved(null);
    try {
      await fn();
    } catch (e) {
      setTest({ ok: false, text: errorText(e) });
    } finally {
      setBusy(null);
    }
  };

  const onTest = () =>
    withBusy("test", async () => {
      setTest(null);
      const msg = await api.testConnection(form.server.host, form.server.ssh_port, form.server.username, password);
      setTest({ ok: true, text: msg });
    });

  const persistPassword = async () => {
    if (password && rememberPw) {
      try {
        await api.storePassword(password);
        setPasswordStored(true);
      } catch (e) {
        // Keychain may be unavailable (e.g. no Secret Service on Linux) — keep going with the in-memory password.
        setTest({ ok: false, text: `Не удалось сохранить пароль в системном хранилище: ${errorText(e)}` });
      }
    }
  };

  const onSave = () =>
    withBusy("save", async () => {
      await save(form);
      await persistPassword();
      setSaved("Настройки сохранены");
    });

  const onConnect = () =>
    withBusy("connect", async () => {
      await save(form);
      await persistPassword();
      await api.connect(form, password);
      setSaved("Подключено к серверу");
    });

  const onForget = () =>
    withBusy("forget", async () => {
      await api.deletePassword();
      setPasswordStored(false);
      setSaved("Пароль удалён из системного хранилища");
    });

  return (
    <div className="max-w-3xl space-y-5">
      <h1 className="text-2xl font-bold">Настройки</h1>

      <Card>
        <CardHeader>
          <CardTitle><Server className="h-4 w-4" />Подключение к Storage Box</CardTitle>
          <Badge variant={status === "connected" ? "success" : "muted"}>{status === "connected" ? "Подключено" : "Не подключено"}</Badge>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="grid grid-cols-3 gap-3">
            <div className="col-span-2">
              <Label>Хост</Label>
              <Input value={form.server.host} onChange={(e) => upd("server", { host: e.target.value.trim() })} placeholder="uXXXXXX.your-storagebox.de" />
            </div>
            <div>
              <Label>SSH порт</Label>
              <select className="select" value={form.server.ssh_port} onChange={(e) => upd("server", { ssh_port: Number(e.target.value) })}>
                <option value={23}>23 (рекомендуется Hetzner)</option>
                <option value={22}>22</option>
              </select>
            </div>
          </div>
          <div className="grid grid-cols-2 gap-3">
            <div>
              <Label>Пользователь</Label>
              <Input value={form.server.username} onChange={(e) => upd("server", { username: e.target.value.trim() })} placeholder="u678016" />
            </div>
            <div>
              <Label>Пароль</Label>
              <div className="relative">
                <Input
                  type={showPw ? "text" : "password"}
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  placeholder={passwordStored ? "•••••• (сохранён в системном хранилище)" : "Пароль Storage Box"}
                  className="pr-10"
                />
                <button
                  type="button"
                  className="absolute right-2 top-1/2 -translate-y-1/2 text-fg-muted hover:text-fg"
                  onClick={() => setShowPw((v) => !v)}
                  title={showPw ? "Скрыть" : "Показать"}
                >
                  {showPw ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
                </button>
              </div>
            </div>
          </div>
          <div className="flex items-center justify-between text-sm">
            <label className="flex items-center gap-2">
              <input type="checkbox" className="h-4 w-4 accent-blue-600" checked={rememberPw} onChange={(e) => setRememberPw(e.target.checked)} />
              Запомнить пароль в системном хранилище (Keychain / Credential Manager)
            </label>
            {passwordStored && (
              <button className="flex items-center gap-1 text-xs text-fg-muted hover:text-red-300" onClick={onForget}>
                <KeyRound className="h-3.5 w-3.5" />Забыть пароль
              </button>
            )}
          </div>
          <div className="flex flex-wrap gap-2">
            <Button variant="secondary" onClick={onTest} disabled={busy !== null}>
              {busy === "test" ? <Loader2 className="h-4 w-4 animate-spin" /> : <Plug className="h-4 w-4" />}Проверить подключение
            </Button>
            {status === "connected" ? (
              <Button variant="outline" onClick={() => withBusy("connect", api.disconnect)} disabled={busy !== null}>
                <Unplug className="h-4 w-4" />Отключиться
              </Button>
            ) : (
              <Button onClick={onConnect} disabled={busy !== null}>
                {busy === "connect" ? <Loader2 className="h-4 w-4 animate-spin" /> : <Plug className="h-4 w-4" />}Подключиться
              </Button>
            )}
          </div>
          {test && (
            <div
              className={`flex items-start gap-2 rounded-lg border p-3 text-sm ${
                test.ok ? "border-success/40 bg-success/10 text-emerald-200" : "border-error/40 bg-error/10 text-red-200"
              }`}
            >
              {test.ok ? <CheckCircle2 className="mt-0.5 h-4 w-4 shrink-0" /> : <XCircle className="mt-0.5 h-4 w-4 shrink-0" />}
              <span className="whitespace-pre-wrap break-all">{test.text}</span>
            </div>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader><CardTitle><HardDrive className="h-4 w-4" />Монтирование диска (rclone)</CardTitle></CardHeader>
        <CardContent className="space-y-4">
          <div className="grid grid-cols-2 gap-3">
            <div>
              <Label>Точка монтирования</Label>
              <Input value={form.mount.mount_point} onChange={(e) => upd("mount", { mount_point: e.target.value })} placeholder="Z: или ~/CloudBox" />
            </div>
            <div>
              <Label>Удалённая папка (пусто = корень)</Label>
              <Input value={form.mount.remote_path} onChange={(e) => upd("mount", { remote_path: e.target.value })} placeholder="/" />
            </div>
          </div>
          <div className="grid grid-cols-3 gap-3">
            <div>
              <Label>Режим кэша</Label>
              <select className="select" value={form.mount.cache_mode} onChange={(e) => upd("mount", { cache_mode: e.target.value })}>
                <option value="off">off — без кэша</option>
                <option value="minimal">minimal</option>
                <option value="writes">writes — кэш записи</option>
                <option value="full">full — полный (рекомендуется)</option>
              </select>
            </div>
            <div>
              <Label>Размер кэша</Label>
              <Input value={form.mount.cache_max_size} onChange={(e) => upd("mount", { cache_max_size: e.target.value })} placeholder="5G" />
            </div>
            <div>
              <Label>Потоков передачи</Label>
              <Input type="number" min={1} max={10} value={form.mount.transfers}
                onChange={(e) => upd("mount", { transfers: Math.min(10, Math.max(1, Number(e.target.value))) })} />
            </div>
          </div>
          <div className="grid grid-cols-2 gap-3">
            <div>
              <Label>Лимит отдачи, КБ/с (0 = без лимита)</Label>
              <Input type="number" min={0} value={form.bandwidth.upload_limit_kbps}
                onChange={(e) => upd("bandwidth", { upload_limit_kbps: Math.max(0, Number(e.target.value)) })} />
            </div>
            <div>
              <Label>Лимит загрузки, КБ/с (0 = без лимита)</Label>
              <Input type="number" min={0} value={form.bandwidth.download_limit_kbps}
                onChange={(e) => upd("bandwidth", { download_limit_kbps: Math.max(0, Number(e.target.value)) })} />
            </div>
          </div>
          <p className="text-xs text-fg-muted">
            Hetzner допускает до 10 одновременных соединений на Storage Box. Для монтирования нужны: Windows — WinFsp, macOS — macFUSE, Linux — FUSE.
          </p>
        </CardContent>
      </Card>

      <Card>
        <CardHeader><CardTitle><Monitor className="h-4 w-4" />Интерфейс</CardTitle></CardHeader>
        <CardContent className="space-y-3 text-sm">
          <label className="flex items-center gap-2">
            <input type="checkbox" className="h-4 w-4 accent-blue-600" checked={form.ui.autostart} onChange={(e) => upd("ui", { autostart: e.target.checked })} />
            Запускать при старте системы
          </label>
          <label className="flex items-center gap-2">
            <input type="checkbox" className="h-4 w-4 accent-blue-600" checked={form.ui.minimize_to_tray} onChange={(e) => upd("ui", { minimize_to_tray: e.target.checked })} />
            Сворачивать в трей при закрытии окна
          </label>
        </CardContent>
      </Card>

      <div className="flex items-center gap-3">
        <Button size="lg" onClick={onSave} disabled={busy !== null}>
          {busy === "save" ? <Loader2 className="h-4 w-4 animate-spin" /> : <Save className="h-4 w-4" />}Сохранить
        </Button>
        {saved && <span className="flex items-center gap-1 text-sm text-emerald-300"><CheckCircle2 className="h-4 w-4" />{saved}</span>}
      </div>
    </div>
  );
}
