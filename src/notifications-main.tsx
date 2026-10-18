import React from "react";
import ReactDOM from "react-dom/client";
import Notifications from "./notifications/Notifications";
import "./noSelect";

document.addEventListener("contextmenu", (e) => e.preventDefault());

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
	<React.StrictMode>
		<Notifications />
	</React.StrictMode>
);
