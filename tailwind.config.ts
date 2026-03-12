import type { Config } from "tailwindcss";

export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  theme: {
    extend: {
      colors: {
        surface: {
          0: "#0a0a0b",
          1: "#111113",
          2: "#18181b",
          3: "#222225",
        },
        accent: {
          DEFAULT: "#6366f1",
          hover: "#818cf8",
        },
        status: {
          running: "#22c55e",
          waiting: "#f59e0b",
          paused: "#6b7280",
          error: "#ef4444",
          disconnected: "#f97316",
          completed: "#3b82f6",
        },
      },
    },
  },
  plugins: [],
} satisfies Config;
