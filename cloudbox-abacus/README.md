# ☁️ CloudBox

Десктопное приложение для работы с **Hetzner Storage Box**: браузер файлов, сетевой диск (rclone mount) и односторонняя синхронизация папок (ПК → облако).

**Стек:** Rust + Tauri 2 · ssh2 (SFTP) · keyring · rusqlite · notify · React 19 + TypeScript · Vite · Tailwind CSS · Zustand · Recharts · rclone.

> MVP: транспорт только SFTP (порт 23), синхронизация только `one-way-up`. SMB и WebDAV запланированы на следующие фазы.

---

## Возможности MVP

| Раздел | Что умеет |
|---|---|
| **Обзор** | Статус соединения и задержка, статус диска, квота (`df` по SSH), график скорости за 60 сек, журнал операций, кнопки «Монтировать диск» / «Синхр. сейчас» |
| **Файлы** | Навигация (хлебные крошки), загрузка (диалог и drag-and-drop), скачивание, новая папка, удаление (рекурсивно), skeleton-загрузка |
| **Синхронизация** | Профили: локальная папка → удалённая, расписание (вручную / в реальном времени через `notify` с debounce 2 с / по интервалу), исключения, прогресс, пауза |
| **Настройки** | Хост, порт 22/23, пользователь, пароль (с «глазом»), проверка подключения, параметры rclone (точка монтирования, кэш, потоки, лимиты скорости), автозапуск, сворачивание в трей |
| **Трей** | «Открыть» / «Синхронизировать» / «Выход», закрытие окна сворачивает в трей |

Безопасность:
- пароль хранится только в системном хранилище (Windows Credential Manager / macOS Keychain / Linux Secret Service);
- в `config.toml` пароля нет;
- rclone получает настройки через переменные окружения `RCLONE_CONFIG_CLOUDBOX_*` (пароль в обфусцированном виде), файл `rclone.conf` не создаётся;
- загрузка идёт во временный `*.cloudbox-part` с атомарным переименованием.

---

## Требования

| Компонент | Версия |
|---|---|
| Rust | 1.77+ (`rustup`) |
| Node.js | 18+ (проверено на 22) |
| rclone | 1.60+ — только для монтирования диска |
| FUSE-драйвер | Windows: [WinFsp](https://winfsp.dev/) · macOS: [macFUSE](https://osxfuse.github.io/) · Linux: `fuse3` |

### Системные зависимости

**Linux (Debian/Ubuntu):**
```bash
sudo apt install -y libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev \
  librsvg2-dev libssl-dev libdbus-1-dev build-essential pkg-config fuse3 rclone
```

**Windows:** Microsoft C++ Build Tools («Desktop development with C++»), WebView2 (есть в Windows 10/11), [WinFsp](https://winfsp.dev/rel/), rclone (`winget install Rclone.Rclone`).

**macOS:** `xcode-select --install`, `brew install rclone`, macFUSE.

---

## Сборка и запуск

```bash
cd cloudbox
npm install

# режим разработки (Vite на :1420 + окно Tauri)
npm run tauri dev

# релизная сборка → target/release/bundle/
npm run tauri build
```

Результаты сборки (Cargo workspace, поэтому каталог `target/` в корне проекта):
- Windows: `target/release/bundle/msi/CloudBox_0.1.0_x64_en-US.msi`, `nsis/CloudBox_0.1.0_x64-setup.exe`
- Linux: `target/release/bundle/deb/CloudBox_0.1.0_amd64.deb`, `appimage/…AppImage`
- macOS: `target/release/bundle/macos/CloudBox.app`, `dmg/…dmg`

Отдельные проверки:
```bash
npm run typecheck                                   # TypeScript
npm run build                                       # сборка фронтенда в dist/
cargo test --manifest-path src-tauri/Cargo.toml     # unit-тесты Rust
```

### Встраивание rclone

CloudBox ищет rclone в таком порядке: рядом с исполняемым файлом (`rclone` / `rclone.exe`), в `binaries/` или `resources/` рядом с ним, затем в `PATH`.
Чтобы положить rclone в установщик, скопируйте бинарник в `src-tauri/binaries/` и добавьте в `tauri.conf.json`:
```json
"bundle": { "resources": { "binaries/rclone.exe": "rclone.exe" } }
```

---

## Первое подключение (Hetzner Storage Box)

1. В Hetzner Console → Storage Box → включите **SSH Support** и **External Reachability** (у BX21 `u678016` уже включены).
2. Запустите CloudBox → **Настройки**:
   - Хост: `u678016.your-storagebox.de`
   - SSH порт: `23`
   - Пользователь: `u678016`
   - Пароль: пароль Storage Box (при необходимости сбрасывается в Hetzner Console)
3. «Проверить подключение» → «Подключиться». С галочкой «Запомнить пароль» при следующем запуске подключение произойдёт автоматически.
4. **Синхронизация** → «Новый профиль» → выберите локальную папку и удалённый путь (например `/backup/documents`).
5. **Обзор** → «Монтировать диск» — Storage Box появится как диск `Z:` (Windows) или `~/CloudBox` (Linux/macOS).

Проверить сервер без GUI:
```bash
sftp -P 23 u678016@u678016.your-storagebox.de
ssh -p 23 u678016@u678016.your-storagebox.de df -h
```

---

## Конфигурация

`~/.cloudbox/config.toml` (создаётся при первом запуске):

```toml
[server]
host = "u678016.your-storagebox.de"
username = "u678016"
ssh_port = 23
protocol = "sftp"

[mount]
enabled = false
mount_point = "/home/user/CloudBox"   # Windows: "Z:"
remote_path = ""
cache_mode = "full"
cache_max_size = "5G"
transfers = 4

[bandwidth]
upload_limit_kbps = 0
download_limit_kbps = 0

[ui]
autostart = false
minimize_to_tray = true
language = "ru"

[[profiles]]
id = "p_docs"
name = "Документы"
local_path = "/home/user/Documents"
remote_path = "/backup/documents"
mode = "one-way-up"
schedule = "realtime"          # manual | realtime | interval
interval_minutes = 60
enabled = true
exclude = [".DS_Store", "Thumbs.db", "*.tmp", "~$*"]
```

Остальные файлы: `~/.cloudbox/cloudbox.db` — SQLite (очередь операций + индекс файлов).

---

## Структура проекта

```
cloudbox/
├── Cargo.toml               # Cargo workspace
├── package.json, vite.config.ts, tailwind.config.js, tsconfig.json
├── src-tauri/
│   ├── tauri.conf.json, capabilities/default.json, icons/
│   └── src/
│       ├── main.rs          # entry point
│       ├── lib.rs           # AppState, Tauri-команды, трей, фоновые задачи
│       ├── config.rs        # Config ↔ TOML
│       ├── secrets.rs       # keyring
│       ├── database.rs      # SQLite: operations, file_index
│       ├── sftp.rs          # SFTP-клиент (ssh2)
│       ├── mount.rs         # rclone mount/unmount
│       ├── sync_engine.rs   # scan → diff → upload → index
│       └── file_watcher.rs  # notify + debounce 2 с
└── src/
    ├── App.tsx, main.tsx, index.css
    ├── components/          # Layout, Dashboard, FileBrowser, SyncProfiles, Settings, SpeedChart, ui/*
    ├── stores/              # connectionStore, syncStore, settingsStore (Zustand)
    ├── hooks/useTauriEvents.ts
    └── lib/                 # api.ts (invoke), types.ts, utils.ts
```

### IPC

Команды: `get_config`, `save_config`, `connect`, `disconnect`, `test_connection`, `get_connection_status`, `get_remote_home`, `list_remote`, `upload_files`, `download_file`, `delete_remote`, `create_remote_dir`, `mount_drive`, `unmount_drive`, `get_mount_status`, `start_sync`, `stop_sync`, `get_sync_status`, `get_operations`, `get_storage_info`, `store_password`, `delete_password`, `check_password_stored`.

События: `sync-progress`, `sync-completed`, `sync-error`, `connection-changed`, `mount-changed`, `speed-update`.

---

## Алгоритм синхронизации (one-way-up)

1. Рекурсивный обход локальной папки с учётом исключений.
2. Для каждого файла: если в `file_index` совпадают размер и mtime → пропуск; иначе `stat` на сервере — если размер совпадает и mtime на сервере не старше → пропуск (индекс обновляется).
3. Загрузка изменённых файлов через `*.cloudbox-part` + rename, перенос mtime на сервер.
4. Запись в журнал `operations`, обновление `file_index`, события прогресса в UI.

Каждая синхронизация открывает своё SFTP-соединение (лимит Hetzner — 10 одновременных), поэтому браузер файлов не блокируется.

---

## Известные ограничения MVP

- Только SFTP и аутентификация по паролю (SSH-ключи, SMB, WebDAV — следующие фазы).
- Удаление локального файла не удаляет его на сервере (режим резервного копирования).
- Скачивание папок целиком пока не поддерживается в браузере файлов (используйте сетевой диск).
- Проверка SSH fingerprint (TOFU) пока не реализована.
