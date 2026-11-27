import React, { useEffect, useRef, useState } from "react";
import * as Slider from "@radix-ui/react-slider";
import { Choice, Icon, ShortcutIcon, useText } from "./components";
import { previewAppearance, commitAppearance } from "./bridge";
import capsule from "../../assets/dock-capsule.json";

const capsuleStyle = {
  "--cap-length": `${capsule.length}px`, "--cap-depth": `${capsule.depth}px`,
  "--cap-side-length": `${capsule.sideLength}px`, "--cap-side-depth": `${capsule.sideDepth}px`,
  "--cap-peek": `${capsule.peek}px`, "--cap-dot": `${capsule.dot}px`, "--cap-gap": `${capsule.gap}px`,
  "--cap-fill": capsule.fill.join(" "),
  "--cap-ink": capsule.ink.join(" "), "--cap-border": capsule.borderAlpha / 255,
};

function DockPreview({ state, style, expanded }) {
  const t = useText();
  const { edge, mode } = state.dock;
  const pins = state.bar.filter(row => row.pinned).slice(0, state.barCapacity);
  const collapse = { top: "up", bottom: "down", left: "back", right: "right" }[edge];
  const margin = style.edgeMargin ?? (mode === "autoHide" ? 0 : 8);
  return <div className="material-preview" aria-hidden="true" data-edge={edge} data-mode={mode} data-open={expanded}
    style={{ ...capsuleStyle, "--opacity": 1 - style.transparency / 100,
      "--bar-length": 2 * style.barPadding + 32 + (64 + style.itemGap) * (pins.length + 1),
      "--bar-padding": style.barPadding + "em", "--item-gap": style.itemGap + "em",
      "--corner-radius": style.cornerRadius + "em", "--edge-margin": margin + "em" }}>
    <div className={'sample-island' + (style.frosted ? ' frosted' : '')}>
      <i/><i/><i/>
      <div className="sample-shortcuts">
        {pins.map(row => <span className="sample-shortcut" key={JSON.stringify(row.key)}>
          <ShortcutIcon row={row}/><span className="sample-label">{row.key.item && !row.directory
            ? row.title.replace(/\.(lnk|url)$/i, "") : row.title}</span>
        </span>)}
        <span className="sample-shortcut"><Icon name="other"/><span className="sample-label">{t("更多", "More")}</span></span>
        <span className="sample-collapse"><Icon name={collapse}/></span>
      </div>
    </div>
  </div>;
}

export default function Appearance({ state, run, saved, failed }) {
  const t = useText();
  const [style, setStyle] = useState(state.appearance);
  const [number, setNumber] = useState(String(style.transparency));
  const latest = useRef(style);
  const dragging = useRef(false);
  const [showExpandedPreview, setShowExpandedPreview] = useState(false);
  const previousCommit = useRef(state.appearance);
  const revision = useRef(0);

  function display(next, normalize = true) {
    latest.current = next;
    setStyle(next);
    if (normalize) setNumber(String(next.transparency));
  }
  useEffect(() => {
    if (!dragging.current && !revision.current) {
      previousCommit.current = state.appearance;
      display(state.appearance);
    }
  }, [state.appearance]);

  function preview(field, value) {
    // Keyboard steps commit directly. Only pointer movement needs a live preview.
    if (!dragging.current) return;
    const next = { ...latest.current, [field]: value };
    display(next);
    previewAppearance(next, failed);
  }
  async function commit(next, normalize = true) {
    dragging.current = false;
    display(next, normalize);
    const token = ++revision.current;
    try {
      await commitAppearance(next);
      previousCommit.current = next;
      if (token === revision.current) saved(next);
    } catch (e) {
      if (token === revision.current) display(previousCommit.current);
      failed(e);
    } finally {
      if (token === revision.current) revision.current = 0;
    }
  }
  function finishNumber() {
    if (!number.trim()) { setNumber(String(latest.current.transparency)); return; }
    const percent = Math.max(0, Math.min(100, Math.round(Number(number))));
    if (Number.isFinite(percent) && percent !== latest.current.transparency) {
      void commit({ ...latest.current, transparency: percent });
    } else { setNumber(String(latest.current.transparency)); }
  }
  function editNumber(event) {
    const raw = event.target.value;
    if (!/^\d{0,3}$/.test(raw)) return;
    setNumber(raw);
    const percent = Number(raw);
    if (raw && percent <= 100 && percent !== latest.current.transparency) {
      void commit({ ...latest.current, transparency: percent }, false);
    }
  }
  function endPointer() {
    if (!dragging.current) return;
    dragging.current = false;
    if (latest.current !== previousCommit.current) {
      void commit(latest.current);
    }
  }
  return <>
    <div className="appearance-preview">
      <DockPreview state={state} style={style} expanded={showExpandedPreview}/>
    </div>
    <section className="sheet" onFocus={e => setShowExpandedPreview(!e.target.closest("#dock-edge, #dock-mode"))}>
      <div className="setting-row"><label htmlFor="dock-edge">{t("停靠位置", "Dock position")}</label>
        <Choice id="dock-edge" label={t("停靠位置", "Dock position")} value={state.dock.edge}
          options={[["top", t("顶部", "Top")], ["bottom", t("底部", "Bottom")],
            ["left", t("左侧", "Left")], ["right", t("右侧", "Right")]]}
          onChange={edge => run({ type: "dockEdge", edge })}/>
      </div>
      <div className="setting-row"><div><label htmlFor="dock-mode">{t("显示方式", "Display mode")}</label>
        <small className="dock-hint">{state.dock.mode === "autoHide"
          ? t("鼠标移到圆弧上即展开快捷栏，移开后自动收起。", "Hover over the visible edge to open the shortcut bar. Move away to hide it.")
          : t("点击胶囊展开快捷栏。", "Click the capsule to open the shortcut bar.")}</small></div>
        <Choice id="dock-mode" label={t("显示方式", "Display mode")} value={state.dock.mode}
          options={[["alwaysVisible", t("常驻", "Always visible")], ["autoHide", t("自动隐藏", "Auto-hide")]]}
          onChange={mode => run({ type: "dockMode", mode })}/>
      </div>
      <div className="setting-row"><label htmlFor="material">{t("背景效果","Background effect")}</label>
        <Choice id="material" label={t("背景效果","Background effect")} value={style.frosted ? "frosted":"clear"}
          options={[["frosted",t("毛玻璃","Frosted glass")],["clear",t("透明","Clear")]]}
          onChange={v => void commit({...latest.current, frosted:v === "frosted"})}/>
      </div>
      <div className="slider-setting"><div className="setting-row">
        <label htmlFor="percent">{t("背景透明度","Background transparency")}</label>
        <div className="percent"><input id="percent" type="text" inputMode="numeric" autoComplete="off" maxLength={3}
          value={number} onFocus={e => e.target.select()} onChange={editNumber} onBlur={finishNumber}
          onKeyDown={e => { if (e.key === "Enter") { finishNumber(); e.currentTarget.select(); } }}/>
          <span>%</span></div>
      </div>
      <Slider.Root className="slider" min={0} max={100} step={1} value={[style.transparency]}
        onPointerDownCapture={() => { dragging.current = true; }}
        onValueChange={([percent]) => preview("transparency", percent)}
        onValueCommit={([percent]) => void commit({ ...latest.current, transparency: percent })}
        onPointerCancel={endPointer} onLostPointerCapture={endPointer} onBlur={endPointer}>
        <Slider.Track className="slider-track"><Slider.Range className="slider-range"/></Slider.Track>
        <Slider.Thumb className="slider-thumb" aria-label={t("背景透明度","Background transparency")} aria-valuetext={style.transparency+"%"}/>
      </Slider.Root>
      <div className="scale"><span>0%</span><span>100%</span></div></div>
    </section>
    <h2 className="section-title">{t("展开布局", "Expanded layout")}</h2>
    <section className="sheet" onFocus={() => setShowExpandedPreview(true)}>
      {[
        ["edgeMargin", t("距屏幕工作区边缘", "Distance from work area edge"), 48],
        ["barPadding", t("快捷栏两端留白", "Toolbar end padding"), 24],
        ["itemGap", t("快捷入口间距", "Shortcut spacing"), 24],
        ["cornerRadius", t("面板圆角", "Panel corner radius"), 32],
      ].map(([field, label, max]) => {
        const value = style[field] ?? (state.dock.mode === "autoHide" ? 0 : 8);
        return <div className="slider-setting" key={field}>
          <div className="setting-row"><span id={field + "-label"}>{label}</span><span>{value} px</span></div>
          <Slider.Root className="slider" min={0} max={max} step={1} value={[value]}
            onPointerDownCapture={() => { dragging.current = true; setShowExpandedPreview(true); }}
            onValueChange={([v]) => preview(field, v)}
            onValueCommit={([v]) => void commit({ ...latest.current, [field]: v })}
            onPointerCancel={endPointer} onLostPointerCapture={endPointer} onBlur={endPointer}>
            <Slider.Track className="slider-track"><Slider.Range className="slider-range"/></Slider.Track>
            <Slider.Thumb className="slider-thumb" aria-labelledby={field + "-label"} aria-valuetext={value + " px"}/>
          </Slider.Root>
        </div>;
      })}
    </section>
  </>;
}
