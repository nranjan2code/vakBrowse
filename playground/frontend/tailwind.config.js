/** @type {import('tailwindcss').Config} */
module.exports = {
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  theme: {
    extend: {
      colors: {
        bg: '#0a0a0f',
        card: '#15151f',
        border: '#2a2a3a',
        accent: '#3b82f6',
        'accent-hover': '#60a5fa',
        text: '#e5e5eb',
        'text-dim': '#8b8b9a',
        danger: '#ef4444',
        success: '#22c581',
      },
    },
  },
  plugins: [],
};
