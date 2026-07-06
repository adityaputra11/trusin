import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Toaster, toast } from "sonner";
import { App } from "./App";
import "./index.css";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: 1,
      refetchOnWindowFocus: false,
      staleTime: 10_000,
    },
    // Surface every mutation failure via toast — previously all mutations
    // failed silently (no onError anywhere in the codebase). Individual hooks
    // keep their onSuccess; this only adds the error leg globally.
    mutations: {
      onError: (err) => {
        const msg = err instanceof Error ? err.message : "Something went wrong";
        toast.error(msg);
      },
    },
  },
});

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <App />
      <Toaster theme="dark" position="bottom-right" richColors closeButton />
    </QueryClientProvider>
  </StrictMode>,
);
