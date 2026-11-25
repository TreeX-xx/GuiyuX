use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq, Debug)]
pub enum Language {
    #[default]
    #[serde(rename = "system")]
    System,
    #[serde(rename = "zh-CN")]
    Chinese,
    #[serde(rename = "en")]
    English,
}

pub struct Text {
    pub categories: &'static str,
    pub groups: [&'static str; 11],
    pub back: &'static str,
    pub collapse: &'static str,
    pub empty: &'static str,
    pub settings: &'static str,
    pub saved: &'static str,
    pub duplicate: &'static str,
    pub quit: &'static str,
    pub language: &'static str,
    pub system: &'static str,
    pub error: &'static str,
}

const ZH: Text = Text {
    categories: "分类",
    groups: [
        "办公通讯",
        "AI 助手",
        "开发工具",
        "设计创作",
        "实用工具",
        "文档",
        "图片",
        "影音",
        "压缩包",
        "文件夹",
        "其他",
    ],
    back: "返回分类",
    collapse: "收起",
    empty: "这个分类还没有收藏",
    settings: "设置",
    saved: "已收纳",
    duplicate: "已收藏",
    quit: "退出",
    language: "界面语言",
    system: "跟随系统",
    error: "无法读写设置或创建窗口",
};
const EN: Text = Text {
    categories: "Categories",
    groups: [
        "Office & chat",
        "AI assistants",
        "Developer tools",
        "Design",
        "Utilities",
        "Documents",
        "Images",
        "Audio & video",
        "Archives",
        "Folders",
        "Other",
    ],
    back: "Back to categories",
    collapse: "Collapse",
    empty: "No items in this category yet",
    settings: "Settings",
    saved: "Saved",
    duplicate: "Already saved",
    quit: "Quit",
    language: "Display language",
    system: "Use system language",
    error: "Could not access settings or create a window",
};

impl Language {
    pub fn is_chinese(self) -> bool {
        match self {
            Self::System => unsafe {
                windows_sys::Win32::Globalization::GetUserDefaultUILanguage() & 0x3ff == 4
            },
            Self::Chinese => true,
            Self::English => false,
        }
    }

    pub fn text(self) -> &'static Text {
        if self.is_chinese() { &ZH } else { &EN }
    }

    pub fn pick(self, zh: &'static str, en: &'static str) -> &'static str {
        if self.is_chinese() { zh } else { en }
    }

    pub fn native_name(self) -> &'static str {
        if self.is_chinese() {
            "简体中文"
        } else {
            "English"
        }
    }
}
