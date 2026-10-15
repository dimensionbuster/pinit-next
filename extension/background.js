// PinIt browser bridge. Right-click any image -> "PinIt으로 열기".
// Talks to the PinIt app through Chrome Native Messaging (no open network port).
// Permissions: contextMenus + nativeMessaging only; this extension never reads pages.

const HOST = "com.pinit.next";
const MENU_ID = "pinit-open";

chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({ id: MENU_ID, title: "PinIt으로 열기", contexts: ["image"] });
  // Lets the PinIt app show "connected" right after install. Ignored if the app is not installed.
  chrome.runtime.sendNativeMessage(HOST, { v: 1, type: "hello" }).catch(() => {});
});

chrome.contextMenus.onClicked.addListener((info, tab) => {
  if (info.menuItemId !== MENU_ID || !info.srcUrl) return;
  void openInPinit(info.srcUrl, info.pageUrl || (tab && tab.url) || "");
});

/** @returns {object|null} native message for a data: URL, or null if unsupported */
function dataUrlMessage(src) {
  const m = /^data:([^;,]+);base64,(.*)$/s.exec(src);
  return m ? { v: 1, type: "open", mime: m[1], data_b64: m[2] } : null;
}

async function openInPinit(srcUrl, pageUrl) {
  let message;
  if (srcUrl.startsWith("data:")) {
    message = dataUrlMessage(srcUrl);
    if (!message) return fail("지원하지 않는 data: 이미지입니다");
  } else if (/^https?:\/\//i.test(srcUrl)) {
    message = { v: 1, type: "open", url: srcUrl, referer: /^https?:\/\//i.test(pageUrl) ? pageUrl : undefined };
  } else {
    return fail("이 형식의 이미지는 열 수 없습니다 (" + srcUrl.split(":")[0] + ":)");
  }

  try {
    const reply = await chrome.runtime.sendNativeMessage(HOST, message);
    if (!reply || !reply.ok) fail((reply && reply.error) || "PinIt이 응답하지 않습니다");
    else clearBadge();
  } catch (e) {
    fail("PinIt이 설치/실행되지 않았거나 연결할 수 없습니다: " + e.message);
  }
}

function fail(text) {
  chrome.action.setBadgeBackgroundColor({ color: "#d6403a" });
  chrome.action.setBadgeText({ text: "!" });
  chrome.action.setTitle({ title: "PinIt — " + text });
  setTimeout(clearBadge, 8000);
}

function clearBadge() {
  chrome.action.setBadgeText({ text: "" });
  chrome.action.setTitle({ title: "PinIt" });
}
