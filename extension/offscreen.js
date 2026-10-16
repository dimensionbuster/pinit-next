// Holds a tab-capture stream open for as long as a PinIt video stream runs. A captured tab
// counts as visible to Chrome, so it keeps rendering (and its <video> keeps painting) even when
// another window completely covers it. The pixels are not read here: frames still come from the
// page's own <video>. Video only, so the tab's audio is not redirected away from the speakers.

/** streamId of the tab capture -> { stream, el } */
const holds = new Map();

chrome.runtime.onMessage.addListener((msg, _sender, reply) => {
  if (!msg || msg.target !== "pinit-offscreen") return;
  if (msg.type === "hold-start") {
    navigator.mediaDevices
      .getUserMedia({ video: { mandatory: { chromeMediaSource: "tab", chromeMediaSourceId: msg.streamId } } })
      .then((stream) => {
        // a sink makes sure the capture is actually consumed (and thus keeps running)
        const el = document.createElement("video");
        el.muted = true;
        el.srcObject = stream;
        void el.play().catch(() => {});
        holds.set(msg.sid, { stream, el });
        reply({ ok: true });
      })
      .catch((e) => reply({ ok: false, error: String((e && e.message) || e) }));
    return true; // async reply
  }
  if (msg.type === "hold-stop") {
    const h = holds.get(msg.sid);
    if (h) {
      h.stream.getTracks().forEach((t) => t.stop());
      h.el.srcObject = null;
      holds.delete(msg.sid);
    }
    reply({ ok: true });
  }
});
