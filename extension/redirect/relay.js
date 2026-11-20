// relay.js - the part of the farglass extension in its isolated world (F040, F052)
//
// It gives the messages of page.js to the service worker, with each append
// in base64, and gives the messages of the agent back to page.js.
//
// relay.js and page.js talk only through a MessagePort (F052). page.js makes
// the MessageChannel and gives one port in a synchronous event, at
// document_start, before any script of the page runs. relay.js takes only the
// first such event. Thus a script of the page cannot send a message to the
// agent: window.postMessage does not reach relay.js.

(() => {
  let port = null;
  let page = null;
  const connect = () => {
    port = chrome.runtime.connect({ name: "player" });
    port.onMessage.addListener((message) => page?.postMessage(message));
    port.onDisconnect.addListener(() => {
      port = null;
    });
  };
  const base64 = (bytes) => {
    if (typeof bytes.toBase64 === "function") return bytes.toBase64();
    let text = "";
    for (let i = 0; i < bytes.length; i += 0x8000) text += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
    return btoa(text);
  };
  const receive = (event) => {
    const message = event.data;
    if (!message || typeof message.type !== "string") return;
    if (message.type === "append") message.data = base64(new Uint8Array(message.data));
    if (!port) connect();
    try {
      port.postMessage(message);
    } catch {
      // The service worker restarted: the next message connects again.
      port = null;
    }
  };
  // Only the first event: page.js sends it before the page runs. A later event is from the page.
  document.addEventListener(
    "farglass-redirect-port",
    (event) => {
      if (page || !event.ports?.[0]) return;
      page = event.ports[0];
      page.onmessage = receive;
    },
    { once: true, capture: true },
  );
})();
