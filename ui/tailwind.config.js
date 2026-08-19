/** @type {import('tailwindcss').Config} */
export default {
  content: [
    './index.html',
    './src/**/*.{js,ts,jsx,tsx}',
  ],
  theme: {
    extend: {
      colors: {
        bg: {
          DEFAULT: '#0f1115',
          panel: '#161922',
          hover: '#1d2030',
        },
        border: {
          DEFAULT: '#262a36',
          accent: '#3b82f6',
        },
        fg: {
          DEFAULT: '#e5e7eb',
          muted: '#9ca3af',
          accent: '#60a5fa',
        },
        success: '#10b981',
        warn: '#f59e0b',
        error: '#ef4444',
      },
      fontFamily: {
        mono: ['"JetBrains Mono"', '"Fira Code"', 'monospace'],
      },
    },
  },
  plugins: [],
};
