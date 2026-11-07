import { render } from "@solidjs/web";
import { App } from "./App.tsx";
import "./styles.css";

const standalone = (navigator as { standalone?: boolean }).standalone || matchMedia("(display-mode: standalone)").matches;
if (standalone) document.documentElement.dataset.standalone = "";

render(() => <App />, document.getElementById("root")!);
