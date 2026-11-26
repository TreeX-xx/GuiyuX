import React, { useEffect, useRef, useState } from "react";
import * as Slider from "@radix-ui/react-slider";
import { Choice, Heading, useText } from "./components";
import { previewAppearance, commitAppearance } from "./bridge";

export default function Appearance({ state, saved, failed }) {
  const t = useText();
  const [style, setStyle] = useState(state.appearance);
  const [number, setNumber] = useState(String(style.transparency));
  const latest = useRef(style);
  const dragging = useRef(false);
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
  }, [state.appearance.frosted, state.appearance.transparency]);

  function preview(percent) {
    // Keyboard steps commit directly. Only pointer movement needs a live preview.
    if (!dragging.current) return;
    const next = { ...latest.current, transparency: percent };
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
    if (latest.current.transparency !== previousCommit.current.transparency) {
      void commit(latest.current);
    }
  }
  return <>
    <Heading title={t("外观","Appearance")}>{t("调整灵动岛和右键菜单的背景效果。","Adjust the background of the island and its context menus.")}</Heading>
    <div className="material-preview" aria-hidden="true">
      <div className={'sample-island' + (style.frosted ? ' frosted' : '')}
        style={{"--opacity": 1 - style.transparency / 100}}><i/><i/><i/></div>
    </div>
    <section className="sheet">
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
        onValueChange={([percent]) => preview(percent)}
        onValueCommit={([percent]) => void commit({ ...latest.current, transparency: percent })}
        onPointerCancel={endPointer} onLostPointerCapture={endPointer} onBlur={endPointer}>
        <Slider.Track className="slider-track"><Slider.Range className="slider-range"/></Slider.Track>
        <Slider.Thumb className="slider-thumb" aria-label={t("背景透明度","Background transparency")} aria-valuetext={style.transparency+"%"}/>
      </Slider.Root>
      <div className="scale"><span>0%</span><span>100%</span></div></div>
    </section>
  </>;
}
