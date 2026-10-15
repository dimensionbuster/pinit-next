import { defineConfig } from "vite";
import { fileURLToPath } from "node:url";

const page = (name: string) => fileURLToPath(new URL(name, import.meta.url));

// Two entry pages: settings window (index.html) and image viewer (viewer.html).
export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**", "**/crates/**", "**/target/**"] },
  },
  build: {
    target: "es2022",
    rollupOptions: {
      input: { main: page("index.html"), viewer: page("viewer.html") },
    },
  },
});
