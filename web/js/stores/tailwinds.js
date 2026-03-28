tailwind.config = {
  darkMode: "class",
  theme: {
    extend: {
      colors: {
        ground: "#111111",
        surface: { DEFAULT: "#1a1a1a" },
        hover: "#222222",
        rule: "#2a2a2a",
        strong: "#3a3a3a",
        primary: "#e5e5e5",
        secondary: "#a3a3a3",
        tertiary: "#737373",
        quaternary: "#525252",
        accent: { DEFAULT: "#60a5fa", light: "#0c1929", rule: "#1e3a5f" },
        success: { DEFAULT: "#4ade80", light: "#0a1f12", rule: "#14532d" },
        danger: { DEFAULT: "#f87171", light: "#1f0a0a", rule: "#5c1a1a" },
        warn: { DEFAULT: "#fbbf24", light: "#1a1608", rule: "#4a3f1a" },
      },
      fontFamily: {
        sans: ['"DM Sans"', "system-ui", "sans-serif"],
        mono: ['"DM Mono"', "ui-monospace", "monospace"],
      },
      fontWeight: {
        400: "400",
        500: "500",
        600: "600",
      },
    },
  },
};
