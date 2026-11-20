// relay.js - the part of the farglass extension in its isolated world (F040)
//
// It gives the messages of page.js to the service worker, with each append
// in base64, and gives the messages of the agent back to page.js.

(() => {
  let port = null;
  const connect = () => {
    port = chrome.runtime.connect({ name: "player" });
    port.onMessage.addListener((message) => window.postMessage({ farglassRedirectBack: message }, "*"));
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
  window.addEventListener("message", (event) => {
    const message = event.source === window ? event.data?.farglassRedirect : undefined;
    if (!message || typeof message.type !== "string") return;
    if (message.type === "append") message.data = base64(new Uint8Array(message.data));
    if (!port) connect();
    try {
      port.postMessage(message);
    } catch {
      // The service worker restarted: the next message connects again.
      port = null;
    }
  });
})();
