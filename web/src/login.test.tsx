import { render } from "@solidjs/web";
import { afterEach, describe, expect, test } from "vitest";
import { App } from "./App";
import { describeSession, promptAutocomplete, promptLabel } from "./login";
import { LoginScreen } from "./LoginScreen";
import { closeFrame, encodeJsonFrame, FrameType, type LoginEvent, parseJson, readyFrame } from "./proto";
import { fakeConnection } from "./proto/testing";

let dispose: (() => void) | undefined;

afterEach(() => {
  dispose?.();
  dispose = undefined;
  document.body.innerHTML = "";
});

function mount(view: () => unknown): HTMLElement {
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(view as () => never, host);
  return host;
}

/** Lets the effects of Solid and the promises run. */
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

function event(channel: number, value: LoginEvent) {
  return encodeJsonFrame(channel, FrameType.Control, value);
}

describe("prompt helpers", () => {
  test.each([
    ["Password: ", "Password"],
    ["login:", "Login"],
    ["Verification code: ", "Verification code"],
    ["", "Answer"],
  ])("promptLabel(%j) is %j", (text, label) => {
    expect(promptLabel(text)).toBe(label);
  });

  test("autocomplete helps the password manager and the one-time code", () => {
    expect(promptAutocomplete("login: ", true)).toBe("username");
    expect(promptAutocomplete("Password: ", false)).toBe("current-password");
    expect(promptAutocomplete("Verification code: ", false)).toBe("one-time-code");
  });
});

describe("LoginScreen", () => {
  test("a passkey step with no passkey for this host name asks for a recovery code (F034)", async () => {
    const { server, connection } = await fakeConnection();
    let loggedIn = "";
    const host = mount(() => <LoginScreen connection={connection} onlogin={(user) => (loggedIn = user)} />);
    server.take();
    server.push(readyFrame(1));
    server.push(event(1, { type: "passkey", elsewhere: ["farglass.cyberdyne.example"] }));
    await settle();
    expect(host.textContent).toContain("Your passkeys belong to farglass.cyberdyne.example");
    expect(host.textContent).not.toContain("Use my passkey");
    const code = host.querySelector<HTMLInputElement>("#login-code")!;
    code.value = "3f9a-0c41-77be-12d0";
    code.closest("form")!.dispatchEvent(new SubmitEvent("submit", { bubbles: true, cancelable: true }));
    await settle();
    expect(parseJson(server.take()[0]!)).toEqual({ type: "recovery-code", code: "3f9a-0c41-77be-12d0" });
    expect(host.querySelector("[role=status]")?.textContent).toBe("Checking");
    server.push(event(1, { type: "success", user: "alice", uid: 1000 }));
    await settle();
    expect(loggedIn).toBe("alice");
  });

  test("a password and then a one-time code, each in its own field, then the success", async () => {
    const { server, connection } = await fakeConnection();
    let loggedIn = "";
    const host = mount(() => <LoginScreen connection={connection} onlogin={(user) => (loggedIn = user)} />);

    const [open] = server.take();
    expect(parseJson(open!)).toEqual({ kind: "login" });
    server.push(readyFrame(1));
    server.push(event(1, { type: "info", text: "Welcome to the test host." }));
    server.push(event(1, { type: "prompt", prompt: 1, text: "Password: ", echo: false }));
    await settle();

    expect(host.textContent).toContain("Welcome to the test host.");
    const password = host.querySelector<HTMLInputElement>("#login-answer")!;
    expect(host.querySelector("label")?.textContent).toBe("Password");
    expect(password.type).toBe("password");
    expect(password.autocomplete).toBe("current-password");

    password.value = "secret";
    password.dispatchEvent(new Event("input", { bubbles: true }));
    host.querySelector("form")!.dispatchEvent(new SubmitEvent("submit", { bubbles: true, cancelable: true }));
    await settle();
    expect(parseJson(server.take()[0]!)).toEqual({ type: "answer", prompt: 1, text: "secret" });
    expect(host.querySelector("[role=status]")?.textContent).toBe("Checking");

    server.push(event(1, { type: "prompt", prompt: 2, text: "Verification code: ", echo: false }));
    await settle();
    const code = host.querySelector<HTMLInputElement>("#login-answer")!;
    expect(code.autocomplete).toBe("one-time-code");
    expect(code.value).toBe("");

    server.push(event(1, { type: "success", user: "alice", uid: 1000 }));
    await settle();
    expect(loggedIn).toBe("alice");
  });

  test("a failure shows the message and a new attempt opens a new channel", async () => {
    const { server, connection } = await fakeConnection();
    const host = mount(() => <LoginScreen connection={connection} onlogin={() => {}} />);
    server.take();
    server.push(readyFrame(1));
    server.push(event(1, { type: "failure", message: "The login failed." }));
    server.push(closeFrame(1, { reason: "done" }));
    await settle();

    expect(host.querySelector("[role=alert]")?.textContent).toBe("The login failed.");
    const retry = [...host.querySelectorAll("button")].find((b) => b.textContent === "Try again")!;
    retry.click();
    await settle();
    const frames = server.take();
    const open = frames.find((frame) => frame.type === FrameType.Open);
    expect(open?.channel).toBe(3);
  });

  test("a block shows the time and disables the new attempt until the time ends", async () => {
    const { server, connection } = await fakeConnection();
    const host = mount(() => <LoginScreen connection={connection} onlogin={() => {}} />);
    server.take();
    server.push(event(1, { type: "blocked", retry_after: 30 }));
    await settle();

    expect(host.querySelector("[role=status]")?.textContent).toBe("Too many failed logins. Try again in 30 s.");
    const retry = [...host.querySelectorAll("button")].find((b) => b.textContent === "Try again")!;
    expect(retry.disabled).toBe(true);
  });

  test("an error of the login service shows its message", async () => {
    const { server, connection } = await fakeConnection();
    const host = mount(() => <LoginScreen connection={connection} onlogin={() => {}} />);
    server.take();
    server.push(closeFrame(1, { reason: "internal-error", message: "The login service is not available." }));
    await settle();
    expect(host.querySelector("[role=alert]")?.textContent).toBe("The login service is not available.");
  });
});

describe("confirm step for a local session", () => {
  test("describeSession names the type and the seat", () => {
    expect(describeSession({ id: "2", kind: "wayland", seat: "seat0" })).toBe("a Wayland session on seat0");
    expect(describeSession({ id: "3", kind: "x11" })).toBe("an X11 session");
  });

  async function confirmScreen() {
    const { server, connection } = await fakeConnection();
    const host = mount(() => <LoginScreen connection={connection} onlogin={() => {}} />);
    server.take();
    server.push(readyFrame(1));
    server.push(event(1, { type: "confirm", sessions: [{ id: "2", kind: "wayland", seat: "seat0" }] }));
    await settle();
    return { server, host };
  }

  test("the screen tells what ends and puts the focus on Cancel", async () => {
    const { host } = await confirmScreen();
    const dialog = host.querySelector("[role=alertdialog]")!;
    expect(dialog.textContent).toContain("a Wayland session on seat0");
    expect(dialog.textContent).toContain("work that is not saved in it is lost");
    expect(document.activeElement?.textContent).toBe("Cancel");
  });

  test.each([
    ["Cancel", false],
    ["End the local session and log in", true],
  ])("the button %j sends accept %s", async (label, accept) => {
    const { server, host } = await confirmScreen();
    [...host.querySelectorAll("button")].find((b) => b.textContent === label)!.click();
    await settle();
    expect(parseJson(server.take()[0]!)).toEqual({ type: "confirm", accept });
    expect(host.querySelector("[role=alertdialog]")).toBeNull();
  });
});

describe("App", () => {
  test("a connection with no user shows the login screen", async () => {
    const { connection } = await fakeConnection();
    const host = mount(() => <App connect={() => Promise.resolve(connection)} />);
    await settle();
    expect(host.querySelector("h1")?.textContent).toContain("Log in to");
  });

  test("a login with a ticket stores the session", async () => {
    const { server, connection } = await fakeConnection();
    const tickets: string[] = [];
    const host = mount(() => (
      <App
        connect={() => Promise.resolve(connection)}
        storeSession={(ticket) => {
          tickets.push(ticket);
          return Promise.resolve(true);
        }}
      />
    ));
    await settle();
    server.take();
    server.push(event(1, { type: "success", user: "alice", uid: 1000, ticket: "t1" }));
    await settle();
    expect(tickets).toEqual(["t1"]);
    expect(host.textContent).toContain("Logged in as alice");
  });

  test("a connection with a user shows the desktop", async () => {
    const { connection } = await fakeConnection("alice");
    const host = mount(() => <App connect={() => Promise.resolve(connection)} />);
    await settle();
    expect(host.textContent).toContain("Logged in as alice");
    expect(host.querySelector("nav[aria-label='Workspaces']")).not.toBeNull();
  });
});

describe("machine PIN (F059)", () => {
  const pinResult = (accepted: boolean, retry_after?: number) =>
    encodeJsonFrame(0, FrameType.Control, { type: "pin-result", accepted, ...(retry_after ? { retry_after } : {}) });

  test("the PIN screen comes first, and no login channel opens before the PIN", async () => {
    const { server, connection } = await fakeConnection(undefined, undefined, true);
    const host = mount(() => <App connect={() => Promise.resolve(connection)} />);
    await settle();
    expect(host.textContent).toContain("PIN of this host");
    expect(server.take().filter((frame) => frame.type === FrameType.Open)).toEqual([]);

    const field = host.querySelector<HTMLInputElement>("#pin-answer")!;
    field.value = "0000";
    host.querySelector("form")!.dispatchEvent(new Event("submit", { cancelable: true }));
    await settle();
    const sent = server.take();
    expect(sent.map((frame) => parseJson(frame))).toEqual([{ type: "pin", pin: "0000" }]);
    server.push(pinResult(false));
    await settle();
    expect(host.querySelector("[role='alert']")?.textContent).toContain("The PIN is not correct.");

    field.value = "4829";
    host.querySelector("form")!.dispatchEvent(new Event("submit", { cancelable: true }));
    await settle();
    server.take();
    server.push(pinResult(true));
    await settle();
    // The login screen opens its login channel.
    expect(server.take().some((frame) => frame.type === FrameType.Open)).toBe(true);
    expect(host.textContent).not.toContain("PIN of this host");
  });

  test("a block disables the field and shows the wait", async () => {
    const { server, connection } = await fakeConnection(undefined, undefined, true);
    const host = mount(() => <App connect={() => Promise.resolve(connection)} />);
    await settle();
    const field = host.querySelector<HTMLInputElement>("#pin-answer")!;
    field.value = "1111";
    host.querySelector("form")!.dispatchEvent(new Event("submit", { cancelable: true }));
    await settle();
    server.push(pinResult(false, 900));
    await settle();
    expect(host.querySelector("[role='alert']")?.textContent).toContain("Try again in 15 minutes.");
    expect(field.disabled).toBe(true);
  });

  test("a host with no PIN shows the login screen at once", async () => {
    const { connection } = await fakeConnection();
    const host = mount(() => <App connect={() => Promise.resolve(connection)} />);
    await settle();
    expect(host.textContent).not.toContain("PIN of this host");
  });
});
