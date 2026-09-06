/** @type {import('tailwindcss').Config} */
module.exports = {
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  darkMode: 'class',
  theme: {
    extend: {
      fontFamily: {
        sans: ['"Space Grotesk"', 'Inter', '-apple-system', 'sans-serif'],
        mono: ['"JetBrains Mono"', '"SF Mono"', 'Menlo', 'monospace'],
      },
      colors: {
        bg: '#0a0a0d',
        card: '#131317',
        surface: '#1a1a20',
        'surface-elevated': '#22222a',
        border: '#2a2a34',
        'border-strong': '#3c3c4a',
        accent: '#ff4500',
        'accent-hover': '#ff5a1a',
        'accent-dim': 'rgba(255, 69, 0, 0.12)',
        yellow: '#ffb800',
        'yellow-dim': 'rgba(255, 184, 0, 0.15)',
        bone: '#eaeae2',
        'bone-dim': '#b8b8ae',
        text: '#f2f2ee',
        'text-dim': '#80808c',
        'text-muted': '#555562',
        danger: '#ff3344',
        success: '#10b981',
      },
      boxShadow: {
        'te-inset': 'inset 0 1px 2px rgba(0,0,0,0.6), inset 0 0 0 1px rgba(255,255,255,0.05)',
        'te-button': '0 2px 0 #08080a, 0 4px 8px rgba(0,0,0,0.4)',
        'te-knob': '0 4px 10px rgba(0,0,0,0.6), inset 0 1px 1px rgba(255,255,255,0.15)',
      },
      borderRadius: {
        xs: '2px',
      },
      backgroundImage: {
        'grid-pattern': 'radial-gradient(circle, #2a2a34 1px, transparent 1px)',
        'millimeter-grid': 'linear-gradient(to right, #1f1f26 1px, transparent 1px), linear-gradient(to bottom, #1f1f26 1px, transparent 1px)',
      },
      backgroundSize: {
        'grid-sm': '16px 16px',
        'grid-mm': '24px 24px',
      },
    },
  },
  plugins: [],
};
