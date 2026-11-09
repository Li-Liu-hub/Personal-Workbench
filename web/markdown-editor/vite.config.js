import { defineConfig } from "vite";

export default defineConfig({
  base: "/",
  build: {
    outDir: "../../assets/markdown-editor",
    emptyOutDir: true,
    cssCodeSplit: false,
    rollupOptions: {
      output: {
        entryFileNames: "editor.js",
        assetFileNames: "[name][extname]",
        chunkFileNames: "editor.js",
      },
    },
  },
});
