import { createEffect, createSignal, onCleanup, Show } from "solid-js";
import type { MenuItem } from "../../menu/menu";
import type { Channel, PtyEvent, PtyRequest } from "../../proto";
import type { AppContext } from "../types";
import type { TerminalEngine } from "./engine";
import { AckCounter, EchoProbe } from "./flow";
import type { TerminalSettings } from "./settings";
import { createXterm } from "./xterm";

const encoder = new TextEncoder();

/** The state of the terminal, as the pane shows it. */
type Status = "starting" | "running" | "exited" | "failed";

/** What the window can do with a pane. */
export interface PaneHandle {
  focus(): void;
  /** Ends the shell of the pane. */
  terminate(): void;
}

export interface PaneProps {
  readonly context: AppContext;
  /** The id of the terminal in the agent. */
  readonly terminal: string;
  /** Reports whether the pane shows: a pane of a tab in the background does not. */
  readonly visible: () => boolean;
  /** The settings of Settings > Terminal, or undefined before they arrive. */
  readonly settings: () => TerminalSettings | undefined;
  /** The pane is ready. The window keeps the handle until the pane goes. */
  readonly onReady: (handle: PaneHandle) => void;
  /** A new title from the program. */
  readonly onTitle: (title: string) => void;
  /**
   * The shell stopped. If the function returns true, the window removes the pane,
   * else the pane offers a new shell.
   */
  readonly onExit: () => boolean;
  /** A key press of the user. Returns true if the window used it (F108). */
  readonly onShortcut: (event: KeyboardEvent) => boolean;
  /** More items for the context menu of the pane, for example the tabs. */
  readonly menuItems: () => MenuItem[];
}

/**
 * One terminal: a shell on the host in an xterm.js terminal.
 *
 * The terminal in the agent continues after a reload, a break of the connection, and in a
 * second browser tab. The new channel attaches to it and gets its scrollback.
 */
export function Pane(props: PaneProps) {
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

  /** Opens the pty channel: attaches to the terminal, or starts it. */
  const attach = () => {
    const term = engine;
    if (!term) {
      return;
    }
    acks?.dispose();
    channel?.close();
    const counter = new AckCounter((bytes) => control({ type: "ack", bytes }));
    acks = counter;
    setStatus("starting");
    channel = context.openChannel(
      { kind: "pty", terminal: props.terminal, cols: term.cols, rows: term.rows },
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
            setStatus("exited");
            if (props.onExit()) return;
            const cause = event.signal !== undefined ? `signal ${event.signal}` : `exit code ${event.code ?? "unknown"}`;
            term.write(`\r\n\x1b[2m[The shell stopped with ${cause}. Press Enter to start a new shell.]\x1b[0m\r\n`);
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
    const term = createXterm(element, props.settings());
    engine = term;
    element.dataset.renderer = term.renderer;
    // The text of the terminal for end-to-end tests and assistive tools: the canvas has no text.
    Object.defineProperty(element, "farglassTerminal", { value: { text: () => term.text(), cols: () => term.cols } });

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
    term.onTitle((title) => props.onTitle(title.trim()));
    term.onShortcut((event) => props.onShortcut(event));
    term.onBlockedCopy((text) => {
      if (replaying === 0) setCopy(text);
    });

    // The size of the pane decides the number of columns and rows. A pane in the background has no size.
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
        ...props.menuItems(),
      ];
    });
    props.onReady({
      focus: () => term.focus(),
      terminate: () => {
        control({ type: "terminate" });
        channel?.close();
      },
    });
    attach();

    onCleanup(() => {
      observer?.disconnect();
      cancelAnimationFrame(frame);
      acks?.dispose();
      channel?.close();
      term.dispose();
    });
  };

  createEffect(
    () => props.settings(),
    (settings) => {
      if (settings) engine?.configure(settings);
    },
  );

  const answerCopy = (accept: boolean) => {
    const text = copy();
    setCopy(null);
    // The click is the action of the user that the clipboard write needs.
    if (accept && text !== null) void navigator.clipboard?.writeText(text);
    engine?.focus();
  };

  // xterm.js uses the class "terminal" for its own element.
  return (
    <div class="term-app" data-status={status()} data-terminal={props.terminal} hidden={!props.visible()} ref={mount}>
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
