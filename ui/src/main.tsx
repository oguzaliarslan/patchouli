import { QueryClientProvider } from "@tanstack/react-query";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { queryClient } from "./lib/queries";
import "./styles.css";

async function start() {
  // Outside the app window (`npm run dev --workspace ui` in a browser), use
  // sample data so the UI can be developed and checked on its own.
  if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) {
    const { installMocks } = await import("./mocks");
    installMocks();
  }
  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <QueryClientProvider client={queryClient}>
        <App />
      </QueryClientProvider>
    </StrictMode>,
  );
}

void start();
