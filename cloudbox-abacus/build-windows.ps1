# ============================================================
#  CloudBox — Build Script для Windows 10/11
#  Запустить от имени Администратора в PowerShell
# ============================================================

Write-Host "=== CloudBox Windows Build Script ===" -ForegroundColor Cyan
Write-Host ""

# Проверка Rust
if (-not (Get-Command "cargo" -ErrorAction SilentlyContinue)) {
    Write-Host "Устанавливаю Rust..." -ForegroundColor Yellow
    Invoke-WebRequest https://win.rustup.rs -OutFile rustup-init.exe
    .\rustup-init.exe -y --default-toolchain stable
    $env:PATH += ";$env:USERPROFILE\.cargo\bin"
    Remove-Item rustup-init.exe
    Write-Host "Rust установлен." -ForegroundColor Green
} else {
    Write-Host "[OK] Rust найден: $(cargo --version)" -ForegroundColor Green
}

# Проверка Node.js
if (-not (Get-Command "node" -ErrorAction SilentlyContinue)) {
    Write-Host ""
    Write-Host "ОШИБКА: Node.js не найден!" -ForegroundColor Red
    Write-Host "Скачайте и установите Node.js LTS с https://nodejs.org/" -ForegroundColor Yellow
    Write-Host "После установки перезапустите этот скрипт." -ForegroundColor Yellow
    exit 1
} else {
    Write-Host "[OK] Node.js найден: $(node --version)" -ForegroundColor Green
}

# Проверка WebView2 Runtime
$webview2 = Get-ItemProperty -Path "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" -ErrorAction SilentlyContinue
if (-not $webview2) {
    Write-Host ""
    Write-Host "Устанавливаю Microsoft WebView2 Runtime..." -ForegroundColor Yellow
    Invoke-WebRequest "https://go.microsoft.com/fwlink/p/?LinkId=2124703" -OutFile webview2.exe
    .\webview2.exe /install
    Remove-Item webview2.exe
    Write-Host "WebView2 установлен." -ForegroundColor Green
} else {
    Write-Host "[OK] WebView2 Runtime найден." -ForegroundColor Green
}

# Установка Visual Studio Build Tools (если нет)
if (-not (Get-Command "cl" -ErrorAction SilentlyContinue)) {
    Write-Host ""
    Write-Host "ВНИМАНИЕ: Visual Studio Build Tools не найдены." -ForegroundColor Yellow
    Write-Host "Если сборка упадёт с ошибкой 'linker not found', скачайте:" -ForegroundColor Yellow
    Write-Host "https://aka.ms/vs/17/release/vs_BuildTools.exe" -ForegroundColor Cyan
    Write-Host "И установите компонент 'Desktop development with C++'" -ForegroundColor Yellow
}

Write-Host ""
Write-Host "Устанавливаю npm зависимости..." -ForegroundColor Yellow
npm install
if ($LASTEXITCODE -ne 0) { Write-Host "ОШИБКА npm install" -ForegroundColor Red; exit 1 }

Write-Host ""
Write-Host "Устанавливаю Tauri CLI..." -ForegroundColor Yellow
cargo install tauri-cli --version "^2" --locked
if ($LASTEXITCODE -ne 0) { Write-Host "ОШИБКА установки tauri-cli" -ForegroundColor Red; exit 1 }

Write-Host ""
Write-Host "Собираю CloudBox для Windows..." -ForegroundColor Yellow
Write-Host "Это займёт 5-15 минут при первой сборке..." -ForegroundColor Gray
Write-Host ""

cargo tauri build --bundles msi,nsis

if ($LASTEXITCODE -eq 0) {
    Write-Host ""
    Write-Host "=== СБОРКА УСПЕШНА! ===" -ForegroundColor Green
    Write-Host ""
    $msi = Get-ChildItem "src-tauri\target\release\bundle\msi\*.msi" -ErrorAction SilentlyContinue
    $exe = Get-ChildItem "src-tauri\target\release\bundle\nsis\*.exe" -ErrorAction SilentlyContinue
    if ($msi) {
        Write-Host "Установщик .msi: $($msi.FullName)" -ForegroundColor Cyan
    }
    if ($exe) {
        Write-Host "Установщик .exe: $($exe.FullName)" -ForegroundColor Cyan
    }
    Write-Host ""
    Write-Host "Дважды кликните на .exe или .msi чтобы установить CloudBox!" -ForegroundColor Green
} else {
    Write-Host ""
    Write-Host "=== ОШИБКА СБОРКИ ===" -ForegroundColor Red
    Write-Host "Проверьте вывод выше для деталей." -ForegroundColor Yellow
}
