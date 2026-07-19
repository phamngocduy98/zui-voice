import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./App";
import "./styles.css";

if (new URLSearchParams(location.search).get("view") === "overlay") {
  document.documentElement.dataset.view = "overlay";
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
