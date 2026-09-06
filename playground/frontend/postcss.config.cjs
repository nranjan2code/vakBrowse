module.exports = {
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  plugins: [],
  // PostCSS needs tailwindcss as a plugin for the build pipeline.
  // This config is consumed by `postcss` in dev and `vite build`.
};
