import React from "react";
import ReactDOM from "react-dom/client";
import NotificationCenter from "./notifications/NotificationCenter";

document.addEventListener("contextmenu", (e) => e.preventDefault());

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
	<React.StrictMode>
		<NotificationCenter />
	</React.StrictMode>
);
