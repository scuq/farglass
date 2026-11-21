import { render } from "@solidjs/web";
import { App } from "./App";
import "./shell.css";
import "./theme/lcars.css";

const root = document.getElementById("root");
if (!root) {
  throw new Error("The page has no #root element.");
}
render(() => <App />, root);
