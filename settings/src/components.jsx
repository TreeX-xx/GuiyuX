import React, { createContext, useContext } from "react";
import * as Select from "@radix-ui/react-select";

export const LanguageContext = createContext("zh-CN");
export function useText() {
  const locale = useContext(LanguageContext);
  return (zh, en) => locale === "zh-CN" ? zh : en;
}

const symbols = {
  collection: 0xe8b7, bar: 0xe8a9, rules: 0xe9e9, appearance: 0xe790, language: 0xe774,
  office: 0xe8bd, assistant: 0xe734, developer: 0xe943, design: 0xe790,
  utilities: 0xe90f, documents: 0xe8a5, images: 0xeb9f, media: 0xe768,
  archives: 0xf012, folders: 0xe8b7, other: 0xe712, right: 0xe76c,
  down: 0xe70d, check: 0xe73e, up: 0xe74a, lower: 0xe74b, back: 0xe72b,
  plus: 0xe710, remove: 0xe74d, pin: 0xe718, file: 0xe8a5, sort: 0xe8cb,
  minimize: 0xe921, maximize: 0xe922, restore: 0xe923, close: 0xe8bb,
};
export function Icon({ name }) {
  return <span aria-hidden="true" className="icon">{String.fromCodePoint(symbols[name] ?? symbols.folders)}</span>;
}

export function ShortcutIcon({ row }) {
  return <Icon name={row.key.group?.builtin || (row.key.group || row.directory ? "folders" : "file")}/>;
}

export function Choice({ id, label, value, options, onChange }) {
  return <Select.Root value={value} onValueChange={onChange}>
    <Select.Trigger id={id} aria-label={label} className="choice">
      <Select.Value /><Select.Icon><Icon name="down" /></Select.Icon>
    </Select.Trigger>
    <Select.Portal>
      <Select.Content position="popper" align="start" sideOffset={5} collisionPadding={8} className="options">
        <Select.Viewport>
          {options.map(([key, text]) => <Select.Item key={key} value={key} className="option">
            <Select.ItemText>{text}</Select.ItemText>
            <Select.ItemIndicator><Icon name="check" /></Select.ItemIndicator>
          </Select.Item>)}
        </Select.Viewport>
      </Select.Content>
    </Select.Portal>
  </Select.Root>;
}

export function ToolButton({ label, icon, ...props }) {
  return <button type="button" className="tool" title={label} aria-label={label} {...props}><Icon name={icon}/></button>;
}
