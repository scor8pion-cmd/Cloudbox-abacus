import { CartesianGrid, Legend, Line, LineChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { formatBytes, formatSpeed } from "../lib/utils";
import { useConnectionStore } from "../stores/connectionStore";

export function SpeedChart({ height = 200 }: { height?: number }) {
  const history = useConnectionStore((s) => s.speedHistory);
  const data = history.map((p, i) => ({ x: i - history.length + 1, upload: p.upload, download: p.download }));

  return (
    <ResponsiveContainer width="100%" height={height}>
      <LineChart data={data} margin={{ top: 8, right: 12, left: 0, bottom: 0 }}>
        <CartesianGrid stroke="#334155" strokeDasharray="3 3" vertical={false} />
        <XAxis dataKey="x" stroke="#64748b" fontSize={11} tickFormatter={(v: number) => `${v}с`} interval={14} />
        <YAxis stroke="#64748b" fontSize={11} width={70} tickFormatter={(v: number) => formatBytes(v, 0)} />
        <Tooltip
          contentStyle={{ background: "#0f172a", border: "1px solid #334155", borderRadius: 8, fontSize: 12 }}
          labelFormatter={(v) => `${v} с`}
          formatter={(v: number, name: string) => [formatSpeed(v), name === "upload" ? "Отдача" : "Загрузка"]}
        />
        <Legend formatter={(v: string) => (v === "upload" ? "Отдача (upload)" : "Загрузка (download)")} wrapperStyle={{ fontSize: 12 }} />
        <Line type="monotone" dataKey="upload" stroke="#3b82f6" strokeWidth={2} dot={false} isAnimationActive={false} />
        <Line type="monotone" dataKey="download" stroke="#10b981" strokeWidth={2} dot={false} isAnimationActive={false} />
      </LineChart>
    </ResponsiveContainer>
  );
}
