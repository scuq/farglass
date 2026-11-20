import { createSignal, onCleanup, Show } from "solid-js";
import type { Channel, PtyEvent, PtyRequest } from "../../proto";
import type { AppContext } from "../types";
import type { TerminalEngine } from "./engine";
import { AckCounter, EchoProbe } from "./flow";
import { createXterm } from "./xterm";
import "./terminal.css";

const encoder = new TextEncoder();

/** The state of the terminal, as the window shows it. */
type Status = "starting" | "running" | "exited" | "failed";

/**
 * The terminal app: a shell on the host in an xterm.js terminal.
 *
 * The terminal in the agent has the id of the window. Thus a reload, a break
 * of the connection, and a second tab attach to the same shell, with its scrollback.
 * Closing the window ends the shell.
 */
export default function TerminalApp(props: { context: AppContext }) {
  const context = props.context;
  const [status, setStatus] = createSignal<Status>("starting");
  let engine: TerminalEngine | undefined;
  let channel: Channel | undefined;
  let acks: AckCounter | undefined;
  const probe = new EchoProbe();
  /** A clipboard write of a program that waits for the user (F055). */
  const [copy, setCopy] = createSignal<string | null>(null);
  /** The bytes of the scrollback replay that the emulator did not parse yet. */
  let replaying = 0;

  const control = (request: PtyRequest) => channel?.control(request);

  /** Opens the pty channel: attaches to the terminal of the window, or starts it. */
  const attach = () => {
    const term = engine;
    if (!term) {
      return;
    }
    acks?.dispose();
    const counter = new AckCounter((bytes) => control({ type: "ack", bytes }));
    acks = counter;
    setStatus("starting");
    channel = context.openChannel(
      { kind: "pty", terminal: context.windowId, cols: term.cols, rows: term.rows },
      {
        ondata: (bytes) => {
          term.write(bytes, () => {
            counter.parsed(bytes.byteLength);
            probe.output();
            replaying = Math.max(0, replaying - bytes.byteLength);
          });
        },
        oncontrol: (message) => {
          const event = message as PtyEvent;
          if (event.type === "attached") {
            setStatus("running");
            // An old clipboard write in the scrollback asks nothing: the program wrote it before.
            replaying = event.replay;
            if (event.replay > 0) {
              // The scrollback can start in the middle of an escape sequence. Start from a clean terminal.
              term.reset();
            }
          } else {
            const cause = event.signal !== undefined ? `signal ${event.signal}` : `exit code ${event.code ?? "unknown"}`;
            term.write(`\r\n\x1b[2m[The shell stopped with ${cause}. Press Enter to start a new shell.]\x1b[0m\r\n`);
            setStatus("exited");
          }
        },
        onclose: (close) => {
          if (status() === "starting") {
            term.write(`\r\n[The terminal did not start: ${close.message ?? close.reason}]\r\n`);
            setStatus("failed");
          }
        },
      },
    );
  };

  const mount = (element: HTMLDivElement) => {
    const term = createXterm(element);
    engine = term;
    element.dataset.renderer = term.renderer;
    // The text of the terminal for end-to-end tests and assistive tools: the canvas has no text.
    Object.defineProperty(element, "farglassTerminal", { value: { text: () => term.text() } });

    term.onInput((data) => {
      if (status() === "exited") {
        if (data === "\r") {
          term.reset();
          attach();
        }
        return;
      }
      probe.input();
      channel?.send(encoder.encode(data));
    });
    term.onResize((cols, rows) => control({ type: "resize", cols, rows }));
    term.onTitle((title) => context.setTitle(title.trim() || "Terminal"));
    term.onBlockedCopy((text) => {
      if (replaying === 0) setCopy(text);
    });

    // The size of the window decides the number of columns and rows.
    let frame = 0;
    const observer =
      typeof ResizeObserver === "undefined"
        ? undefined
        : new ResizeObserver(() => {
            cancelAnimationFrame(frame);
            frame = requestAnimationFrame(() => term.fit());
          });
    observer?.observe(element);
    term.fit();

    context.onFocus(() => term.focus());
    context.contextMenu(element, () => {
      const selection = term.selection();
      return [
        {
          label: "Copy",
          shortcut: "Ctrl+Shift+C",
          disabled: selection === "",
          action: () => void navigator.clipboard?.writeText(selection),
        },
        { label: "Paste", shortcut: "Ctrl+Shift+V", action: () => term.pasteClipboard() },
        { separator: true },
        { label: "Select all", action: () => term.selectAll() },
        { label: "Clear the scrollback", action: () => term.clear() },
      ];
    });
    // Closing the window ends the shell. A reload or a break of the connection does not.
    context.onClose(() => control({ type: "terminate" }));
    if (context.isFocused()) {
      term.focus();
    }
    attach();

    onCleanup(() => {
      observer?.disconnect();
      cancelAnimationFrame(frame);
      acks?.dispose();
      term.dispose();
    });
  };

  const answerCopy = (accept: boolean) => {
    const text = copy();
    setCopy(null);
    // The click is the action of the user that the clipboard write needs.
    if (accept && text !== null) void navigator.clipboard?.writeText(text);
    engine?.focus();
  };

  // xterm.js uses the class "terminal" for its own element.
  return (
    <div class="term-app" data-status={status()} ref={mount}>
      <Show when={copy()}>
        {(text) => (
          <div class="term-copy" role="alertdialog" aria-label="Clipboard write">
            <span>
              A program in this terminal wants to copy {text().length === 1 ? "1 character" : `${text().length} characters`} to your clipboard.
            </span>
            <button type="button" onClick={() => answerCopy(true)}>
              Copy
            </button>
            <button type="button" onClick={() => answerCopy(false)}>
              Ignore
            </button>
          </div>
        )}
      </Show>
    </div>
  );
}
