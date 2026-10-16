// PinIt browser bridge. Right-click any image -> "PinIt으로 열기".
// Talks to the PinIt app through Chrome Native Messaging (no open network port).
// Permissions: contextMenus + nativeMessaging, plus activeTab + scripting for the click-to-stream
// video feature (below). No host access; a page is touched only after you click.

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

// ---------------------------------------------------------------------------
// Live video: toolbar button (or right-click on a video) streams the playing <video>
// of the tab into a PinIt window. Frames are JPEG snapshots sent through Native
// Messaging; nothing is read from the page except the video itself, and only after
// you click (activeTab). DRM-protected video shows black.
// ---------------------------------------------------------------------------

const VIDEO_MENU_ID = "pinit-video";
/** sid -> { native, tabId, frameId, framePort, started } */
const streams = new Map();

chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({ id: VIDEO_MENU_ID, title: "PinIt으로 영상 띄우기", contexts: ["video"] });
});

chrome.action.onClicked.addListener((tab) => {
  if (tab.id != null) void startVideo(tab);
});

chrome.contextMenus.onClicked.addListener((info, tab) => {
  if (info.menuItemId === VIDEO_MENU_ID && tab && tab.id != null) void startVideo(tab, info.frameId);
});

async function startVideo(tab, frameId) {
  const target = frameId != null ? { tabId: tab.id, frameIds: [frameId] } : { tabId: tab.id, allFrames: true };
  let best;
  try {
    const found = await chrome.scripting.executeScript({ target, func: pinitVideo, args: ["", false] });
    best = found.filter((r) => r.result).sort((a, b) => b.result.area - a.result.area)[0];
  } catch (e) {
    return fail("이 페이지에서는 영상을 가져올 수 없습니다 (" + e.message + ")");
  }
  if (!best) return fail("재생 중인 영상을 찾지 못했습니다");

  const sid = crypto.randomUUID();
  const native = chrome.runtime.connectNative(HOST);
  streams.set(sid, { native, tabId: tab.id, frameId: best.frameId, framePort: null, started: false });
  native.onMessage.addListener((m) => onNativeMessage(sid, m));
  native.onDisconnect.addListener(() => {
    const err = chrome.runtime.lastError;
    const s = streams.get(sid);
    if (err && s && !s.started) fail("PinIt에 연결할 수 없습니다: " + err.message);
    endStream(sid);
  });
  native.postMessage({ v: 1, type: "video-start", sid, title: tab.title || "", page: tab.url || "" });
}

function onNativeMessage(sid, m) {
  const s = streams.get(sid);
  if (!s || !m) return;
  if (m.ok === false) {
    fail(m.error || "PinIt이 응답하지 않습니다");
    endStream(sid);
  } else if (m.type === "video-ready" && !s.started) {
    s.started = true;
    clearBadge();
    chrome.scripting
      .executeScript({ target: { tabId: s.tabId, frameIds: [s.frameId] }, func: pinitVideo, args: [sid, true] })
      .catch((e) => {
        fail("영상을 캡처할 수 없습니다 (" + e.message + ")");
        endStream(sid);
      });
  } else if (m.type === "stop") {
    endStream(sid);
  }
}

function endStream(sid) {
  const s = streams.get(sid);
  if (!s) return;
  streams.delete(sid);
  try {
    if (s.framePort) {
      s.framePort.postMessage({ stop: true });
      s.framePort.disconnect();
    }
  } catch (_) {}
  try {
    s.native.disconnect();
  } catch (_) {}
}

// The injected capture script connects back here and pushes frames.
chrome.runtime.onConnect.addListener((port) => {
  if (!port.name.startsWith("pinit-video:")) return;
  const sid = port.name.slice("pinit-video:".length);
  const s = streams.get(sid);
  if (!s) return port.disconnect();
  s.framePort = port;
  port.onMessage.addListener((m) => {
    if (m.type === "video-error") {
      fail(m.error || "영상을 캡처할 수 없습니다");
      endStream(sid);
    } else {
      s.native.postMessage({ v: 1, sid, type: m.type, w: m.w, h: m.h, jpeg_b64: m.jpeg_b64 });
    }
  });
  port.onDisconnect.addListener(() => endStream(sid));
});

/**
 * Runs inside the page (serialised by executeScript, so it must stay self-contained).
 * start=false: report the best <video> of this frame. start=true: capture it.
 */
function pinitVideo(sid, start) {
  let best = null;
  let bestScore = 0;
  for (const v of document.querySelectorAll("video")) {
    if (v.readyState < 2 || v.videoWidth === 0 || v.ended) continue;
    const r = v.getBoundingClientRect();
    const visible =
      Math.max(0, Math.min(r.right, innerWidth) - Math.max(r.left, 0)) *
      Math.max(0, Math.min(r.bottom, innerHeight) - Math.max(r.top, 0));
    const score = visible + (v.paused ? 0 : 1e9); // a playing video beats a bigger paused one
    if (score > bestScore) {
      bestScore = score;
      best = v;
    }
  }
  if (!best) return null;
  if (!start) return { area: bestScore };

  const MAX_W = 1280;
  const FPS = 24;
  const QUALITY = 0.72;
  const video = best;
  const port = chrome.runtime.connect({ name: "pinit-video:" + sid });
  const canvas = document.createElement("canvas");
  const ctx = canvas.getContext("2d");
  let stopped = false;
  let busy = false;
  let last = 0;
  const stop = () => {
    if (stopped) return;
    stopped = true;
    try {
      port.disconnect();
    } catch (_) {}
  };
  port.onMessage.addListener((m) => m && m.stop && stop());
  port.onDisconnect.addListener(() => (stopped = true));

  const step = () => {
    if (stopped) return;
    if (!video.isConnected || video.ended) {
      port.postMessage({ type: "video-stop" });
      stop();
      return;
    }
    const now = performance.now();
    if (!busy && !video.paused && video.readyState >= 2 && video.videoWidth && now - last >= 1000 / FPS) {
      last = now;
      busy = true;
      const scale = Math.min(1, MAX_W / video.videoWidth);
      const w = Math.round(video.videoWidth * scale);
      const h = Math.round(video.videoHeight * scale);
      if (canvas.width !== w || canvas.height !== h) {
        canvas.width = w;
        canvas.height = h;
      }
      try {
        ctx.drawImage(video, 0, 0, w, h);
        canvas.toBlob(
          (blob) => {
            if (!blob) {
              busy = false;
              return;
            }
            const reader = new FileReader();
            reader.onload = () => {
              busy = false;
              if (!stopped) port.postMessage({ type: "video-frame", w, h, jpeg_b64: String(reader.result).split(",")[1] });
            };
            reader.onerror = () => (busy = false);
            reader.readAsDataURL(blob);
          },
          "image/jpeg",
          QUALITY,
        );
      } catch (e) {
        port.postMessage({ type: "video-error", error: "이 영상은 캡처할 수 없습니다 (" + e.name + ")" });
        stop();
        return;
      }
    }
    if (typeof video.requestVideoFrameCallback === "function") video.requestVideoFrameCallback(step);
    else requestAnimationFrame(step);
  };
  step();
  return true;
}
