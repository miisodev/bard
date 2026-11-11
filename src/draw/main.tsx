import { holdForNativePickers } from "../hold";
import React from "react";
import ReactDOM from "react-dom/client";
import { boot } from "../dev/boot";
import "../styles.css";
import "./draw.css";

holdForNativePickers();

void boot("draw").then(async () => {
  const { default: DrawWindow } = await import("./DrawWindow");
  ReactDOM.createRoot(document.getElementById("root")!).render(
    <React.StrictMode>
      <DrawWindow />
    </React.StrictMode>,
  );
});
