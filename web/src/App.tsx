import { Echo } from "./Echo";

/** The root of the shell. In M0, it has only the echo panel. */
export function App() {
  return (
    <main class="shell">
      <Echo />
    </main>
  );
}
