import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { boot } from "./dev/boot";
import "./styles.css";
// The licence card and the confirmation primitive are defined in the settings
// stylesheet but also render inside the panel, via onboarding and the panel's
// own licence gate.
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
