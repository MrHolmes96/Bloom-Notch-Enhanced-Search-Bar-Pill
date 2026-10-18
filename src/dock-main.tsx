import React from "react";
import ReactDOM from "react-dom/client";
import Dock from "./Dock";
import "./Dock.css";
import "./noSelect";

document.addEventListener("contextmenu", (e) => e.preventDefault());

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
	<React.StrictMode>
		<Dock />
	</React.StrictMode>
);
