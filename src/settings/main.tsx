import React from "react";
import ReactDOM from "react-dom/client";
import { boot } from "../dev/boot";
import "../styles.css";
import "./settings.css";

void boot("settings").then(async () => {
  const { default: SettingsWindow } = await import("./SettingsWindow");
  ReactDOM.createRoot(document.getElementById("root")!).render(
    <React.StrictMode>
      <SettingsWindow />
    </React.StrictMode>,
  );
});
