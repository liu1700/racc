import type { Config } from "tailwindcss";
import typography from "@tailwindcss/typography";

export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  theme: {
    extend: {
      colors: {
        surface: {
          0: "#121215",
          1: "#1a1a1f",
          2: "#232329",
          3: "#2e2e35",
        },
        accent: {
          DEFAULT: "#6366f1",
          hover: "#818cf8",
        },
        status: {
          running: "#22c55e",
          waiting: "#f59e0b",
          error: "#ef4444",
          disconnected: "#f97316",
          completed: "#3b82f6",
        },
      },
      animation: {
        "status-pulse":
          "status-pulse 2s cubic-bezier(0.4, 0, 0.6, 1) infinite",
        "fade-out": "fade-out 5s ease-out forwards",
      },
      keyframes: {
        "status-pulse": {
          "0%, 100%": { opacity: "1" },
          "50%": { opacity: "0.4" },
        },
        "fade-out": {
          "0%": { opacity: "1" },
          "80%": { opacity: "1" },
          "100%": { opacity: "0" },
        },
      },
    },
  },
  plugins: [typography],
} satisfies Config;
