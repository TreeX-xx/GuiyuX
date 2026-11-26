import React, { useCallback, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import logo from "../../assets/guiyuX.ico";
import { Icon, LanguageContext, ToolButton, useText } from "./components";
import { finishRequests, request } from "./bridge";
import Appearance from "./appearance";
import Shortcuts from "./shortcuts";
import Rules from "./rules";
import "./styles.css";

const pages = {
  collection: { title: ["收纳", "Collection"], description: ["管理灵动岛中的收藏和分类。", "Manage the items and groups in your island."] },
  bar: { title: ["快捷栏", "Shortcut bar"], description: ["选择固定入口，调整它们在灵动岛上的顺序。", "Choose which shortcuts appear on the island and in what order."] },
  rules: { title: ["整理规则", "Sorting rules"], description: ["规则从上到下匹配。手动指定的分类优先。", "Rules match from top to bottom. Manually assigned groups take priority."] },
  appearance: { title: ["外观", "Appearance"], description: ["调整灵动岛和右键菜单的背景效果。", "Adjust the background of the island and its context menus."] },
  language: { title: ["界面语言", "Language"], description: ["选择灵动岛、菜单和设置使用的语言。", "Choose the language for the island, menus, and settings."] },
};

// Window behavior is documented in .agents/notes/module.md#设置窗口与常驻核心.
function WindowControls({ windowHandle, failed }) {
  const t = useText();
  const [maximized, setMaximized] = useState(false);
  useEffect(() => {
    let active = true;
    function refresh() {
      if (active) windowHandle.isMaximized().then(value => { if (active) setMaximized(value); }).catch(failed);
    }
    const subscription = windowHandle.onResized(refresh);
    subscription.then(refresh, failed);
    return () => { active = false; subscription.then(stop => stop(), () => {}); };
  }, [windowHandle, failed]);

  return <div className="window-controls" role="group" aria-label={t("窗口操作", "Window controls")}>
    <ToolButton icon="minimize" label={t("最小化", "Minimize")} onClick={() => windowHandle.minimize().catch(failed)}/>
    <ToolButton icon={maximized ? "restore" : "maximize"} label={maximized ? t("还原", "Restore") : t("最大化", "Maximize")}
      onClick={() => windowHandle.toggleMaximize().catch(failed)}/>
    <ToolButton icon="close" label={t("关闭", "Close")} onClick={() => windowHandle.close().catch(failed)}/>
  </div>;
}

function Collection({ state, run }) {
  const t = useText();
  return <>
    <section className="sheet">
      <label className="setting-row switch-row">
        <span>{t("自动整理新收藏", "Sort new items automatically")}</span>
        <input type="checkbox" role="switch" checked={state.autoSort}
          onChange={e => run({ type: "autoSort", enabled: e.target.checked })}/><span className="switch" aria-hidden="true"/>
      </label>
      <div className="setting-row"><span>{t("桌面图标", "Desktop icons")}</span>
        <div className="actions wrap"><button className="primary" onClick={() => run({ type: "organize" })}>{t("立即整理", "Organize now")}</button>
          <button onClick={() => run({ type: "restoreDesktop" })}>{t("恢复桌面图标", "Restore desktop icons")}</button></div>
      </div>
      <p className="desktop-hint">{t("整理会收纳桌面的文件、文件夹和快捷方式，并收起包括回收站在内的所有桌面图标。文件保留在原位置。", "Organizing collects desktop files, folders and shortcuts, then hides all desktop icons, including the Recycle Bin. Files stay at their original paths.")}</p>
    </section>
    <div className="section-title"><h2>{t("分类与文件夹", "Categories and folders")}</h2>
      <button onClick={() => run({ type: "showGroup", group: { builtin: "other" } })}>{t("查看其他", "View Other")}</button></div>
    <div className="groups">{state.groups.map(({ key, name, count }) =>
      <button key={JSON.stringify(key)} className="group" onClick={() => run({ type: "showGroup", group: key })}>
        <Icon name={key.builtin || "folders"}/><span>{name}</span><small>{count}</small>
      </button>)}</div>
  </>;
}

function Language({ state, run }) {
  const t = useText();
  return <>
    <div className="sheet" role="radiogroup" aria-label={t("界面语言", "Language")}>
      {[["system", t("跟随系统", "Use system language")], ["zh-CN", "简体中文"], ["en", "English"]].map(([value, name]) =>
        <label className="setting-row language-row" key={value}>
          <input type="radio" name="language" value={value} checked={state.language === value}
            onChange={() => run({ type: "language", language: value })}/>
          <span>{name}</span>{value === "system" && <small>{state.systemLanguage}</small>}
        </label>)}
    </div>
  </>;
}

function Settings() {
  const [windowHandle] = useState(getCurrentWindow);
  const [state, setState] = useState(null);
  const [page, setPage] = useState("collection");
  const [status, setStatus] = useState("");
  const [error, setError] = useState("");
  const [attempt, setAttempt] = useState(0);
  const locale = state?.locale ?? (navigator.language.startsWith("zh") ? "zh-CN" : "en");
  const t = (zh, en) => locale === "zh-CN" ? zh : en;
  const failed = useCallback(e => setError(String(e)), []);

  useEffect(() => {
    let active = true, unsubscribe;
    async function start() {
      const stop = await listen("core-change", async ({ payload }) => {
        try {
          const { state } = await request({ type: "snapshot" });
          if (!active) return;
          setState(state);
          if (payload.event === "organized" || payload.event === "desktopRestored") {
            setPage("collection");
            setStatus(payload.message);
          }
        } catch (e) { if (active) failed(e); }
      });
      if (!active) { stop(); return; }
      unsubscribe = stop;
      const result = await request({ type: "initial" });
      if (active) {
        setState(result.state);
        setStatus(result.notice || "");
      }
    }
    start().catch(e => { if (active) failed(e); });
    return () => { active = false; unsubscribe?.(); };
  }, [attempt, failed]);

  const ready = Boolean(state || error);
  useEffect(() => {
    if (!ready) return;
    const title = locale === "zh-CN" ? "GuiyuX 设置" : "GuiyuX Settings";
    document.documentElement.lang = locale;
    document.title = title;
    const frame = requestAnimationFrame(() => invoke("present", { title }).catch(failed));
    return () => cancelAnimationFrame(frame);
  }, [ready, locale, failed]);

  useEffect(() => {
    const close = windowHandle.onCloseRequested(finishRequests);
    close.catch(failed);
    function escape(event) {
      if (event.key === "Escape" && !event.defaultPrevented && !document.querySelector('[role="listbox"][data-state="open"],dialog[open]')) {
        windowHandle.close().catch(failed);
      }
    }
    window.addEventListener("keydown", escape);
    return () => {
      window.removeEventListener("keydown", escape);
      close.then(unlisten => unlisten()).catch(failed);
    };
  }, [windowHandle, failed]);

  async function run(action, notice = "") {
    try {
      setError("");
      const result = await request(action);
      if (result?.state) setState(result.state);
      setStatus(result?.message || notice);
      return result;
    } catch (e) { failed(e); return null; }
  }
  function navigate(id) {
    setPage(id);
    setError("");
    setStatus("");
  }
  return <LanguageContext.Provider value={locale}>
    <div className="layout">
      <aside><div className="brand" data-tauri-drag-region><img src={logo} alt="" draggable="false"/><strong>GuiyuX</strong></div>
        <nav aria-label={t("设置页面", "Settings pages")}>{Object.entries(pages).map(([id, { title }]) =>
          <button key={id} aria-current={page === id ? "page" : undefined} onClick={() => navigate(id)}>
            <Icon name={id}/>{t(...title)}
          </button>)}</nav>
      </aside>
      <main>
        <header className="page-heading" data-tauri-drag-region>
          <h1>{t(...pages[page].title)}</h1>
          <WindowControls windowHandle={windowHandle} failed={failed}/>
          <p>{t(...pages[page].description)}</p>
        </header>
        <div className="page-content" key={page}>
          {state && page === "collection" && <Collection state={state} run={run}/>}
          {state && page === "bar" && <Shortcuts state={state} run={run} failed={failed}/>}
          {state && page === "rules" && <Rules state={state} run={run}/>}
          {state && page === "appearance" && <Appearance state={state} failed={failed}
            saved={appearance => setState(s => ({ ...s, appearance }))}/>}
          {state && page === "language" && <Language state={state} run={run}/>}
          <div role="status" className="notice">{status}</div>
          {error && <div role="alert" className="error">{error}</div>}
          {!state && error && <button onClick={() => { setError(""); setAttempt(n => n + 1); }}>{t("重试", "Retry")}</button>}
        </div>
      </main>
    </div>
  </LanguageContext.Provider>;
}

createRoot(document.getElementById("root")).render(<Settings/>);
