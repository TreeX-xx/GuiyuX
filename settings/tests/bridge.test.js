import assert from "node:assert/strict";
import test from "node:test";
import { commitAppearance, finishRequests, previewAppearance, request } from "../src/bridge.js";

test("releasing the slider drops obsolete previews and closing waits for the final save", async t => {
  const sent = [];
  const started = Promise.withResolvers();
  const resume = Promise.withResolvers();
  globalThis.window = { __TAURI_INTERNALS__: {
    async invoke(command, { action }) {
      assert.equal(command, "request");
      sent.push(action);
      if (sent.length === 1) {
        started.resolve();
        await resume.promise;
      }
    },
  } };
  t.after(() => { delete globalThis.window; });
  const style = transparency => ({ frosted: true, transparency });
  previewAppearance(style(12), assert.fail);
  await started.promise;
  previewAppearance(style(34), assert.fail);
  previewAppearance(style(56), assert.fail);
  const saved = commitAppearance(style(78));
  const closing = finishRequests();
  assert.equal(sent.length, 1);
  resume.resolve();
  await closing;
  await saved;
  assert.deepEqual(sent.map(a => [a.style.transparency, a.commit]), [[12, false], [78, true]]);
});

test("a failed request does not block the next user action", async t => {
  globalThis.window = { __TAURI_INTERNALS__: {
    async invoke(_command, { action }) {
      if (action.type === "saveRule") throw new Error("Invalid rule");
      return action.type;
    },
  } };
  t.after(() => { delete globalThis.window; });
  const failed = request({ type: "saveRule" });
  const next = request({ type: "snapshot" });
  await assert.rejects(failed, /Invalid rule/);
  assert.equal(await next, "snapshot");
});
