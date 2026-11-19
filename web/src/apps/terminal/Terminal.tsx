import { createSignal, onCleanup } from "solid-js";
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
          });
        },
        oncontrol: (message) => {
          const event = message as PtyEvent;
          if (event.type === "attached") {
            setStatus("running");
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

  // xterm.js uses the class "terminal" for its own element.
  return <div class="term-app" data-status={status()} ref={mount} />;
}
