/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      // Hex values mirror the CSS variables in index.css (hex is required for /opacity modifiers).
      colors: {
        bg: { primary: "#0f172a", secondary: "#1e293b", card: "#1e293b" },
        border: "#334155",
        fg: { DEFAULT: "#f1f5f9", muted: "#94a3b8" },
        accent: "#2563eb",
        success: "#10b981",
        warning: "#f59e0b",
        error: "#ef4444",
      },
      keyframes: {
        shimmer: { "0%": { backgroundPosition: "-400px 0" }, "100%": { backgroundPosition: "400px 0" } },
      },
      animation: { shimmer: "shimmer 1.4s linear infinite" },
    },
  },
  plugins: [],
};
