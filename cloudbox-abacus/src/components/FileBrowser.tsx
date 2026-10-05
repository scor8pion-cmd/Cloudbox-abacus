import { useCallback, useEffect, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { ChevronRight, Download, File, FileArchive, FileImage, FileText, FileVideo, Folder, FolderPlus, Home, Loader2, RefreshCw, Trash2, Upload, UploadCloud } from "lucide-react";
import { Button } from "./ui/button";
import { Card } from "./ui/card";
import { Input } from "./ui/input";
import { api } from "../lib/api";
import type { FileEntry } from "../lib/types";
import { cn, errorText, formatBytes, formatUnixTime, isTauri } from "../lib/utils";
import { useConnectionStore } from "../stores/connectionStore";

function iconFor(e: FileEntry) {
  if (e.is_dir) return <Folder className="h-4 w-4 text-amber-400" />;
  const ext = e.name.split(".").pop()?.toLowerCase() ?? "";
  if (["jpg", "jpeg", "png", "gif", "webp", "heic", "svg"].includes(ext)) return <FileImage className="h-4 w-4 text-pink-400" />;
  if (["mp4", "mkv", "mov", "avi", "webm"].includes(ext)) return <FileVideo className="h-4 w-4 text-purple-400" />;
  if (["zip", "tar", "gz", "7z", "rar", "zst"].includes(ext)) return <FileArchive className="h-4 w-4 text-orange-400" />;
  if (["txt", "md", "pdf", "doc", "docx", "csv", "log"].includes(ext)) return <FileText className="h-4 w-4 text-sky-400" />;
  return <File className="h-4 w-4 text-fg-muted" />;
}

const parentOf = (p: string) => {
  const t = p.replace(/\/+$/, "");
  const i = t.lastIndexOf("/");
  return i <= 0 ? "/" : t.slice(0, i);
};
const joinPath = (base: string, name: string) => `${base.replace(/\/+$/, "")}/${name}`;

export function FileBrowser() {
  const connected = useConnectionStore((s) => s.status === "connected");
  const [home, setHome] = useState("/");
  const [path, setPath] = useState<string | null>(null);
  const [entries, setEntries] = useState<FileEntry[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [dragging, setDragging] = useState(false);
  const [newFolder, setNewFolder] = useState<string | null>(null);

  const load = useCallback(async (p: string) => {
    setLoading(true);
    setError(null);
    try {
      setEntries(await api.listRemote(p));
      setPath(p);
      setSelected(new Set());
    } catch (e) {
      setError(errorText(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (!connected) return;
    api.getRemoteHome().then((h) => { setHome(h); load(h); }).catch((e) => setError(errorText(e)));
  }, [connected, load]);

  const uploadPaths = useCallback(
    async (paths: string[]) => {
      if (!path || !paths.length) return;
      setBusy("upload");
      setError(null);
      try {
        await api.uploadFiles(paths, path);
      } catch (e) {
        setError(errorText(e));
      } finally {
        setBusy(null);
        load(path);
      }
    },
    [path, load],
  );

  // Native drag-and-drop gives real file system paths in Tauri.
  useEffect(() => {
    if (!isTauri || !connected) return;
    const un = getCurrentWebview().onDragDropEvent((event) => {
      const t = event.payload.type;
      if (t === "enter" || t === "over") setDragging(true);
      else if (t === "leave") setDragging(false);
      else if (t === "drop") {
        setDragging(false);
        uploadPaths(event.payload.paths);
      }
    });
    return () => { un.then((f) => f()); };
  }, [connected, uploadPaths]);

  const onUpload = async () => {
    const res = await open({ multiple: true, title: "Выберите файлы для загрузки" });
    if (res) uploadPaths(Array.isArray(res) ? res : [res]);
  };

  const onDownload = async () => {
    const files = entries.filter((e) => selected.has(e.path) && !e.is_dir);
    if (!files.length) return setError("Выберите файл(ы) для скачивания (папки пока не поддерживаются)");
    setBusy("download");
    setError(null);
    try {
      if (files.length === 1) {
        const target = await save({ defaultPath: files[0].name, title: "Сохранить файл" });
        if (target) await api.downloadFile(files[0].path, target);
      } else {
        const dir = await open({ directory: true, title: "Папка для сохранения" });
        if (typeof dir === "string") {
          for (const f of files) await api.downloadFile(f.path, `${dir}/${f.name}`);
        }
      }
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(null);
    }
  };

  const onDelete = async () => {
    const items = [...selected];
    if (!items.length || !path) return;
    if (!window.confirm(`Удалить ${items.length} элемент(ов)? Это действие необратимо.`)) return;
    setBusy("delete");
    try {
      for (const p of items) await api.deleteRemote(p);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(null);
      load(path);
    }
  };

  const onCreateFolder = async () => {
    if (!path || !newFolder?.trim()) return;
    try {
      await api.createRemoteDir(joinPath(path, newFolder.trim()));
      setNewFolder(null);
      load(path);
    } catch (e) {
      setError(errorText(e));
    }
  };

  const toggle = (p: string, multi: boolean) =>
    setSelected((s) => {
      const next = new Set(multi ? s : []);
      if (s.has(p) && multi) next.delete(p);
      else next.add(p);
      return next;
    });

  if (!connected) {
    return (
      <div className="flex h-full flex-col items-center justify-center text-fg-muted">
        <UploadCloud className="mb-3 h-12 w-12" />
        <div>Нет подключения к серверу. Подключитесь в разделе «Настройки».</div>
      </div>
    );
  }

  // Breadcrumbs relative to the remote home.
  const rel = path && path.startsWith(home) ? path.slice(home.length) : path ?? "";
  const crumbs = rel.split("/").filter(Boolean);

  return (
    <div className="flex h-full flex-col gap-4">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">Файлы</h1>
        <div className="flex gap-2">
          <Button size="sm" onClick={onUpload} disabled={busy !== null}>
            {busy === "upload" ? <Loader2 className="h-4 w-4 animate-spin" /> : <Upload className="h-4 w-4" />}Загрузить
          </Button>
          <Button size="sm" variant="secondary" onClick={onDownload} disabled={busy !== null || !selected.size}>
            {busy === "download" ? <Loader2 className="h-4 w-4 animate-spin" /> : <Download className="h-4 w-4" />}Скачать
          </Button>
          <Button size="sm" variant="secondary" onClick={() => setNewFolder("")} disabled={busy !== null}>
            <FolderPlus className="h-4 w-4" />Новая папка
          </Button>
          <Button size="sm" variant="destructive" onClick={onDelete} disabled={busy !== null || !selected.size}>
            <Trash2 className="h-4 w-4" />Удалить
          </Button>
          <Button size="icon" variant="ghost" onClick={() => path && load(path)} title="Обновить">
            <RefreshCw className={cn("h-4 w-4", loading && "animate-spin")} />
          </Button>
        </div>
      </div>

      <div className="flex items-center gap-1 text-sm">
        <button className="flex items-center gap-1 rounded px-2 py-1 text-fg-muted hover:bg-slate-700/40 hover:text-fg" onClick={() => load(home)}>
          <Home className="h-3.5 w-3.5" />Storage Box
        </button>
        {crumbs.map((c, i) => (
          <span key={i} className="flex items-center gap-1">
            <ChevronRight className="h-3.5 w-3.5 text-slate-600" />
            <button className="rounded px-2 py-1 hover:bg-slate-700/40" onClick={() => load(joinPath(home, crumbs.slice(0, i + 1).join("/")))}>
              {c}
            </button>
          </span>
        ))}
      </div>

      {error && <div className="rounded-lg border border-error/40 bg-error/10 p-3 text-sm text-red-200 whitespace-pre-wrap">{error}</div>}

      {newFolder !== null && (
        <div className="flex gap-2">
          <Input autoFocus placeholder="Имя новой папки" value={newFolder} onChange={(e) => setNewFolder(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter") onCreateFolder(); if (e.key === "Escape") setNewFolder(null); }} />
          <Button onClick={onCreateFolder}>Создать</Button>
          <Button variant="ghost" onClick={() => setNewFolder(null)}>Отмена</Button>
        </div>
      )}

      <Card className={cn("relative flex-1 overflow-hidden", dragging && "ring-2 ring-accent")}>
        {dragging && (
          <div className="absolute inset-0 z-10 flex flex-col items-center justify-center bg-blue-950/70 text-blue-200">
            <UploadCloud className="mb-2 h-10 w-10" />Отпустите, чтобы загрузить в {path}
          </div>
        )}
        <div className="h-full overflow-y-auto">
          <table className="w-full text-sm">
            <thead className="sticky top-0 bg-slate-800/95 text-left text-xs text-fg-muted">
              <tr>
                <th className="px-4 py-2.5 font-medium">Имя</th>
                <th className="w-28 px-4 py-2.5 text-right font-medium">Размер</th>
                <th className="w-40 px-4 py-2.5 font-medium">Изменён</th>
              </tr>
            </thead>
            <tbody>
              {loading && !entries.length
                ? Array.from({ length: 8 }).map((_, i) => (
                    <tr key={i} className="border-t border-border/40">
                      <td className="px-4 py-3"><div className="skeleton h-4 w-64" /></td>
                      <td className="px-4 py-3"><div className="skeleton ml-auto h-4 w-14" /></td>
                      <td className="px-4 py-3"><div className="skeleton h-4 w-28" /></td>
                    </tr>
                  ))
                : (
                  <>
                    {path && path !== "/" && path !== home && (
                      <tr className="cursor-pointer border-t border-border/40 hover:bg-slate-700/30" onDoubleClick={() => load(parentOf(path))}>
                        <td className="px-4 py-2 text-fg-muted" colSpan={3}><span className="flex items-center gap-2"><Folder className="h-4 w-4" />..</span></td>
                      </tr>
                    )}
                    {entries.map((e) => (
                      <tr
                        key={e.path}
                        className={cn("cursor-pointer border-t border-border/40 hover:bg-slate-700/30", selected.has(e.path) && "bg-accent/15")}
                        onClick={(ev) => toggle(e.path, ev.ctrlKey || ev.metaKey)}
                        onDoubleClick={() => e.is_dir && load(e.path)}
                      >
                        <td className="px-4 py-2"><span className="flex items-center gap-2">{iconFor(e)}<span className="truncate">{e.name}</span></span></td>
                        <td className="px-4 py-2 text-right text-fg-muted">{e.is_dir ? "—" : formatBytes(e.size)}</td>
                        <td className="px-4 py-2 text-fg-muted">{formatUnixTime(e.modified)}</td>
                      </tr>
                    ))}
                    {!entries.length && !loading && (
                      <tr><td colSpan={3} className="py-16 text-center text-fg-muted">Папка пуста. Перетащите файлы сюда для загрузки.</td></tr>
                    )}
                  </>
                )}
            </tbody>
          </table>
        </div>
      </Card>
      <div className="text-xs text-fg-muted">Двойной клик — открыть папку · Ctrl+клик — выбрать несколько · Перетащите файлы для загрузки</div>
    </div>
  );
}
