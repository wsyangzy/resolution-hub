import { createRoot } from "react-dom/client";

import App from "./App";
import { tauriApi } from "./lib/tauri";
import "./styles/index.css";

const root = document.getElementById("root");

if (!root) {
  throw new Error("缺少 React 挂载节点");
}

void tauriApi.initializeWindowSizePersistence().finally(() => {
  createRoot(root).render(<App />);
});
