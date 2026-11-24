import { MotionConfig } from "motion/react";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { Toaster } from "sonner";
import App from "./App";
import { TooltipProvider } from "./components/ui/tooltip";
import "./index.css";

// Disable default browser context menu & Ctrl+Wheel zoom for native desktop feel.
if (typeof window !== "undefined") {
  window.addEventListener("contextmenu", (e) => {
    const target = e.target as HTMLElement | null;
    if (target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA")) {
      return;
    }
    e.preventDefault();
  });
  window.addEventListener(
    "wheel",
    (e) => {
      if (e.ctrlKey) {
        e.preventDefault();
      }
    },
    { passive: false },
  );
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <MotionConfig reducedMotion="user">
      <TooltipProvider delayDuration={300}>
        <App />
        <Toaster
          theme="dark"
          position="bottom-right"
          toastOptions={{
            style: {
              background: "#14141b",
              border: "1px solid rgba(255,255,255,0.12)",
              color: "#f5f5f7",
            },
          }}
        />
      </TooltipProvider>
    </MotionConfig>
  </StrictMode>,
);
