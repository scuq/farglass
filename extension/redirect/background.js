// background.js - the service worker of the farglass extension (F040)
//
// It gives the messages of each tab to the native messaging host of farglass,
// with the id of the tab, and gives the messages of the agent to their tab.

const HOST = "org.farglass.redirect";
let native = null;
const tabs = new Map();

function host() {
  if (!native) {
    native = chrome.runtime.connectNative(HOST);
    native.onMessage.addListener((message) => {
      for (const port of tabs.get(message.tab) ?? []) port.postMessage(message);
    });
    native.onDisconnect.addListener(() => {
      console.warn("farglass: the native messaging host stopped", chrome.runtime.lastError?.message);
      native = null;
    });
  }
  return native;
}

chrome.runtime.onConnect.addListener((port) => {
  if (port.name !== "player") return;
  const tab = port.sender?.tab?.id ?? -1;
  if (!tabs.has(tab)) tabs.set(tab, new Set());
  tabs.get(tab).add(port);
  port.onMessage.addListener((message) => {
    try {
      host().postMessage({ ...message, tab });
    } catch {
      native = null;
    }
  });
  port.onDisconnect.addListener(() => {
    tabs.get(tab)?.delete(port);
    if (tabs.get(tab)?.size === 0) {
      tabs.delete(tab);
      try {
        host().postMessage({ type: "tab-closed", tab });
      } catch {
        native = null;
      }
    }
  });
});
