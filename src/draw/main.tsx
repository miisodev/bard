import React from "react";
import ReactDOM from "react-dom/client";
import DrawWindow from "./DrawWindow";
import "../styles.css";
import "./draw.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <DrawWindow />
  </React.StrictMode>
);
