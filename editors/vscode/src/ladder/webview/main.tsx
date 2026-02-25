import React from "react";
import { createRoot } from "react-dom/client";
import { LadderEditor } from "./LadderEditor";
import "./styles.css";

/**
 * Entry point for the Ladder editor webview
 */
const container = document.getElementById("root");

if (!container) {
  throw new Error("Root element not found");
}

const root = createRoot(container);
root.render(<LadderEditor />);
