import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import "./styles.css";
// The licence card and the confirmation primitive are defined in the settings
// stylesheet but also render inside the panel, via onboarding and the panel's
// own licence gate. Without this the panel showed an unstyled 14px label where
// a screen-reader-only label belonged, and an unstyled row of buttons where a
// confirmation belonged.
import "./settings/settings.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>
);