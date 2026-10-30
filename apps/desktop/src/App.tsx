import { PROTOCOL_VERSION } from '@hearme/shared';

/**
 * Placeholder shell for the Hearme desktop UI. Later prompts add the live
 * transcript window, library and consent flow. Colours come from the "Quiet
 * Signal" palette (see docs/design/palette.md and styles.css).
 */
export function App() {
  return (
    <main className="app">
      <h1 className="app__title">Hearme</h1>
      <p className="app__tagline">
        Botless meeting transcription. Nothing has been built here yet — this is the app shell.
      </p>
      <p className="app__meta">Gateway protocol v{PROTOCOL_VERSION}</p>
    </main>
  );
}
