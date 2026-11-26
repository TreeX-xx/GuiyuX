import React, { useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Icon, ToolButton, useText } from "./components";

function editorFor(row) {
  return {
    selected: JSON.stringify(row.key),
    name: row.key.group?.folder != null ? row.title : "",
  };
}

export default function Shortcuts({ state, run, failed }) {
  const t = useText();
  const [editor, setEditor] = useState(() => editorFor(state.bar[0]));
  const { selected, name } = editor;
  const confirm = useRef(null);
  const entry = state.bar.find(e => JSON.stringify(e.key) === selected) ?? state.bar[0];
  const index = state.bar.indexOf(entry);
  const pinned = state.bar.filter(e => e.pinned).length;
  const folder = entry?.key.group?.folder;
  const saved = t("快捷栏已保存", "Shortcuts saved");
  async function edit(action) {
    const result = await run(action, saved);
    if (result?.selection || (result && action.type === "deleteFolder")) {
      const row = result.state.bar.find(e => JSON.stringify(e.key) === JSON.stringify(result.selection)) ?? result.state.bar[0];
      const next = editorFor(row);
      // A response may arrive while the user starts typing the next folder name.
      setEditor(current => current.selected === selected ? {
        selected: next.selected, name: current.name === name ? next.name : current.name,
      } : current);
    }
  }
  async function pick(directory, replace = null) {
    try {
      const path = await open({ directory, multiple: false });
      if (typeof path === "string") await edit({ type: "pinPath", path, replace });
    } catch (e) { failed(e); }
  }
  return <>
    <div className="section-title"><h2>{t("快捷入口", "Shortcuts")}</h2><div className="actions">
      <ToolButton icon="up" label={t("上移", "Move up")} disabled={!entry?.pinned || index === 0}
        onClick={() => edit({ type: "moveShortcut", key: entry.key, down: false })}/>
      <ToolButton icon="lower" label={t("下移", "Move down")} disabled={!entry?.pinned || index + 1 >= pinned}
        onClick={() => edit({ type: "moveShortcut", key: entry.key, down: true })}/>
      <button disabled={!entry} onClick={() => edit({ type: "pin", key: entry.key, pinned: !entry.pinned })}>
        {entry?.pinned ? t("取消固定", "Unpin") : t("固定到快捷栏", "Pin to bar")}</button>
    </div></div>
    <div className="entry-list" aria-label={t("快捷入口", "Shortcuts")}>
      {state.bar.map(row => <button className="entry" key={JSON.stringify(row.key)} aria-pressed={entry === row}
        title={row.key.item || row.title} onClick={() => setEditor(editorFor(row))}>
        <Icon name={row.key.group?.builtin || (row.key.group || row.directory ? "folders" : "file")}/>
        <span>{row.title}{row.key.item && <small>{row.key.item}</small>}</span>
        {row.pinned && <span aria-label={t("已固定", "Pinned")}><Icon name="pin"/></span>}
      </button>)}
    </div>
    <h2>{t("收纳文件夹", "Collection folders")}</h2>
    <form className="actions wrap" onSubmit={e => { e.preventDefault(); if (name.trim()) edit({ type: "createFolder", name }); }}>
      <input className="grow" aria-label={t("文件夹名称", "Folder name")} placeholder={t("文件夹名称", "Folder name")}
        value={name} onChange={e => setEditor({ selected, name: e.target.value })}/>
      <button disabled={!name.trim()}>{t("新建", "Create")}</button>
      <button type="button" disabled={folder == null || !name.trim()} onClick={() => edit({ type: "renameFolder", id: folder, name })}>{t("重命名", "Rename")}</button>
      <button type="button" disabled={folder == null} onClick={() => confirm.current.showModal()}>{t("删除", "Delete")}</button>
    </form>
    <h2>{t("本地路径", "Local paths")}</h2>
    <div className="actions wrap">
      <button onClick={() => pick(false)}>{t("固定文件…", "Pin file…")}</button>
      <button onClick={() => pick(true)}>{t("固定目录…", "Pin directory…")}</button>
      <button disabled={!entry?.directory} onClick={() => pick(true, entry.key)}>{t("更换目录…", "Change directory…")}</button>
    </div>
    <dialog ref={confirm} aria-labelledby="delete-folder-title"><h2 id="delete-folder-title">{t("删除收纳文件夹？", "Delete collection folder?")}</h2>
      <p>{t("其中的收藏将移到「其他」。", "Its items will move to Other.")}</p>
      <form method="dialog" className="actions"><button autoFocus>{t("取消", "Cancel")}</button>
        <button className="danger" onClick={() => edit({ type: "deleteFolder", id: folder })}>{t("删除", "Delete")}</button>
      </form>
    </dialog>
  </>;
}
