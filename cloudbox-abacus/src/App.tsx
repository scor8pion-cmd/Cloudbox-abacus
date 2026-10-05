import { useEffect, useState } from "react";
import { Layout, type Page } from "./components/Layout";
import { Dashboard } from "./components/Dashboard";
import { FileBrowser } from "./components/FileBrowser";
import { SyncProfiles } from "./components/SyncProfiles";
import { Settings } from "./components/Settings";
import { useTauriEvents } from "./hooks/useTauriEvents";
import { useSettingsStore } from "./stores/settingsStore";
import { api } from "./lib/api";
import { isTauri } from "./lib/utils";

export default function App() {
  const [page, setPage] = useState<Page>("dashboard");
  const load = useSettingsStore((s) => s.load);
  useTauriEvents();

  useEffect(() => {
    if (!isTauri) return;
    // Load config and auto-connect if a password is stored in the keychain.
    load()
      .then(async () => {
        const { config, passwordStored } = useSettingsStore.getState();
        if (passwordStored) await api.connect(config, "").catch(() => {});
        else setPage("settings");
      })
      .catch(console.error);
  }, [load]);

  return (
    <Layout page={page} onNavigate={setPage}>
      {page === "dashboard" && <Dashboard onNavigate={setPage} />}
      {page === "files" && <FileBrowser />}
      {page === "sync" && <SyncProfiles />}
      {page === "settings" && <Settings />}
    </Layout>
  );
}
