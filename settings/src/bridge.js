import { invoke } from "@tauri-apps/api/core";

// Keep edits, previews and commits in input order across the two processes.
let tail = Promise.resolve();
export function request(action) {
  const next = tail.then(() => invoke("request", { action }));
  tail = next.catch(() => {});
  return next;
}

let pendingPreview;
let sendingPreview = false;
export function previewAppearance(style, onError) {
  pendingPreview = style;
  if (sendingPreview) return;
  sendingPreview = true;
  async function flush() {
    try {
      while (pendingPreview) {
        const latest = pendingPreview;
        pendingPreview = undefined;
        await request({ type: "appearance", style: latest, commit: false });
      }
    } catch (error) {
      pendingPreview = undefined;
      onError(error);
    } finally {
      sendingPreview = false;
    }
  }
  void flush();
}

export function commitAppearance(style) {
  pendingPreview = undefined;
  return request({ type: "appearance", style, commit: true });
}

export function finishRequests() {
  pendingPreview = undefined;
  return tail;
}
