import React, { useRef, useState } from "react";
import { Choice, ToolButton, useText } from "./components";

const emptyRule = { kind: "extension", pattern: "", category: "documents" };

export default function Rules({ state, run }) {
  const t = useText();
  const [index, setIndex] = useState(state.rules.length ? 0 : null);
  const [draft, setDraft] = useState(state.rules[0] ?? emptyRule);
  const [changes, setChanges] = useState([]);
  const preview = useRef(null);
  const pattern = useRef(null);
  const kinds = [["extension", t("扩展名", "Extension")], ["name", t("完整文件名", "File name")], ["application", t("应用标识", "Application ID")]];
  const categories = state.groups.filter(g => g.key.builtin).map(g => [g.key.builtin, g.name]);
  const categoryTitle = Object.fromEntries(categories);
  function select(i, rules = state.rules) {
    setIndex(i);
    setDraft(i == null ? emptyRule : rules[i]);
  }
  function change(part) { setDraft(previous => ({ ...previous, ...part })); }
  async function move(down) {
    const r = await run({ type: "moveRule", index, down });
    if (r) setIndex(r.index);
  }
  async function remove() {
    const r = await run({ type: "deleteRule", index });
    if (r) select(r.state.rules.length ? Math.min(index, r.state.rules.length - 1) : null, r.state.rules);
  }
  async function save(organize) {
    const r = await run({ type: "saveRule", index, rule: draft, organize }, t("规则已保存", "Rule saved"));
    if (r) select(r.index, r.state.rules);
  }
  async function showPreview() {
    const r = await run({ type: "previewRule", index, rule: draft });
    if (r && preview.current) {
      setChanges(r.changes);
      preview.current.showModal();
    }
  }
  return <>
    <div className="section-title"><h2>{t("优先顺序", "Priority")}</h2>
      <div className="actions">
        <ToolButton icon="up" label={t("上移", "Move up")} disabled={index == null || index === 0} onClick={() => move(false)}/>
        <ToolButton icon="lower" label={t("下移", "Move down")} disabled={index == null || index >= state.rules.length - 1} onClick={() => move(true)}/>
        <ToolButton icon="remove" label={t("删除规则", "Delete rule")} disabled={index == null} onClick={remove}/>
        <button onClick={() => { select(null); pattern.current.focus(); }}>{t("新建规则", "New rule")}</button>
      </div>
    </div>
    {state.rules.length ? <div className="entry-list rule-list">
      {state.rules.map((rule, i) => <button key={i} className="entry" aria-pressed={i === index} onClick={() => select(i)}>
        <span className="rule-position">{i + 1}</span><span>{rule.pattern}</span><small>{categoryTitle[rule.category]}</small>
      </button>)}
    </div> : <p className="empty">{t("目前使用内置分类规则。", "Built-in rules are currently in use.")}</p>}
    <h2>{index == null ? t("新规则", "New rule") : t("编辑规则", "Edit rule")}</h2>
    <form className="rule-form" onSubmit={e => { e.preventDefault(); if (draft.pattern.trim()) save(false); }}>
      <div className="rule-fields">
        <div className="field"><label htmlFor="kind">{t("匹配方式", "Match by")}</label>
          <Choice id="kind" label={t("匹配方式", "Match by")} value={draft.kind} options={kinds} onChange={kind => change({ kind })}/>
        </div>
        <div className="field"><label htmlFor="pattern">{t("匹配值", "Match value")}</label>
          <input id="pattern" ref={pattern} value={draft.pattern} onChange={e => change({ pattern: e.target.value })}
            placeholder={{ extension: ".drawio", name: "README.md", application: "Code.exe" }[draft.kind]}/>
        </div>
      </div>
      <div className="field"><label htmlFor="target">{t("目标分类", "Target category")}</label>
        <Choice id="target" label={t("目标分类", "Target category")} value={draft.category} options={categories} onChange={category => change({ category })}/>
      </div>
      <div className="actions wrap"><button type="button" disabled={!draft.pattern.trim()} onClick={showPreview}>{t("预览变化", "Preview changes")}</button>
        <button disabled={!draft.pattern.trim()}>{t("保存规则", "Save rule")}</button>
        <button className="primary" type="button" disabled={!draft.pattern.trim()} onClick={() => save(true)}>{t("保存并整理", "Save and organize")}</button>
      </div>
    </form>
    <dialog ref={preview} aria-labelledby="preview-title"><h2 id="preview-title">{t("预览变化", "Preview changes")}</h2>
      <p>{t("「保存并整理」会应用以下分类并收起全部桌面图标。", "Save and organize applies these categories and hides all desktop icons.")}</p>
      {changes.length ? <ul className="changes">{changes.map(row => <li key={row.path}><span>{row.path}</span>
        <small>{row.from} → {row.to}</small></li>)}</ul> : <p>{t("没有待收纳或调整的项目。", "There are no new items or category changes.")}</p>}
      <form method="dialog"><button autoFocus>{t("返回编辑", "Back to editing")}</button></form>
    </dialog>
  </>;
}
