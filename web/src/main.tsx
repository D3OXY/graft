import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

function App() {
  return (
    <main>
      <h1>Graft Hub</h1>
      <p>The embedded Hub interface is coming online.</p>
    </main>
  );
}

const root = document.getElementById("root");
if (!root) throw new Error("Graft Hub root element is missing");
createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
