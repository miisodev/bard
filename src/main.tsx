import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { boot } from "./dev/boot";
import "./styles.css";
// The settings stylesheet carries the buttons' focus ring and icon sizing,
// which the panel's buttons share.
import "./settings/settings.css";

// App is imported after boot: its modules read the window's identity when they
// load, and in the browser preview that identity is only there once mocked.
void boot("main").then(async () => {
  const { default: App } = await import("./App");
  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
});
