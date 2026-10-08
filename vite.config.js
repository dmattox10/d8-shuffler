import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri expects a fixed port, and its own output should not be buried by Vite's.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: { target: "safari15" },
});
