import React from "react";
import ReactDOM from "react-dom/client";
import Launcher from "./launcher/Launcher";
import "./noSelect";

document.addEventListener("contextmenu", (e) => e.preventDefault());

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
	<React.StrictMode>
		<Launcher />
	</React.StrictMode>
);
