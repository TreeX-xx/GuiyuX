# GuiyuX

Windows 桌面收纳工具。Rust 常驻进程用 Win32 绘制灵动岛，设置窗口按需使用 Tauri 2、WebView2 和 React/CSS。点击主屏顶边的胶囊展开分类栏，点击右端上箭头或岛外可收起。从资源管理器拖入文件或文件夹可保存引用，进入分类后可通过 Windows 打开条目。右键胶囊选择「设置…」可管理收纳、快捷栏、整理规则、外观和界面语言。选择「退出」结束程序。

## 编译与运行

开发环境使用 Windows x64、Rust stable MSVC 工具链、Visual Studio C++ Build Tools、Windows SDK，以及 Node.js 22.12+ 和 npm。构建脚本按两个 Cargo.lock 和前端的 package-lock.json 安装依赖，将网页资源嵌入设置程序。运行设置窗口需要 Microsoft Edge WebView2 Runtime。

```powershell
.\build.ps1
.\target\release\guiyux.exe
```

运行和分发时，将同次构建的 `guiyux.exe` 与 `guiyux-settings.exe` 放在同一目录，启动 `guiyux.exe`。打开设置时才会启动第二个程序，关闭窗口后它随 WebView2 一起退出。调试构建使用 `.\build.ps1 -Profile Debug`。若需要另存一份，使用 `-OutputDirectory <目录>`。脚本在本机临时目录编译，核心的 Cargo 缓存位于 `%LOCALAPPDATA%\GuiyuX-build\cargo-target`，设置宿主的缓存位于 `target\settings-cargo`。

## 模块维护

| 入口 | 职责 |
| --- | --- |
| `src/main.rs` | 加载设置、单实例锁、切换语言、启动和退出 |
| `src/config.rs` | 配置模型、默认值和同目录临时文件替换写入 |
| `src/collection.rs` | 收藏归属、手动放置、快捷栏顺序及批量整理 |
| `src/collection/classify.rs` | 应用标识、快捷方式目标、域名及文件类型的规则匹配 |
| `src/language.rs` | 语言标识、系统语言解析和原生界面的双语文案 |
| `src/ui/mod.rs` | Win32 消息循环、窗口注册、控件创建 |
| `src/ui/island.rs` | 胶囊收展、文件投放、分类导航与分页浏览与系统打开动作 |
| `src/ui/island/backdrop.rs` | 随岛面收展的实时桌面模糊与圆角裁切 |
| `src/ui/menu.rs` | 胶囊右键菜单、键盘选取和退出入口 |
| `src/ui/menu/frame.rs` | 菜单外沿和悬停高亮的透明圆角绘制 |
| `src/ui/settings.rs`、`src/ui/settings/actions.rs` | 启动或唤回设置窗口，通过匿名管道接收请求，在核心线程应用设置和写入配置 |
| `settings/host/` | Tauri 2 设置宿主、窗口生命周期、命令桥接和系统文件选择器 |
| `settings/src/` | React 设置界面、双语文案、CSS 及支持键盘操作的下拉框和滑杆 |
| `src/ui/paint.rs` | 界面配色、离屏绘制、文字与圆角合成 |

用户数据位于 `%LOCALAPPDATA%\GuiyuX\settings.json`。设置窗口按需创建，关闭时销毁。灵动岛静止时等待窗口消息。默认提供 11 个分类。快捷栏默认显示六个入口，顺序可在设置中调整。点「更多」会在同一灵动岛的快捷栏下方展开分类网格，再点一次可折回单行。图标使用无边框线形符号，鼠标移入时，图标绕自身中心轻摆一次后回正。

- **外观**同时控制灵动岛和右键菜单，默认使用 30% 毛玻璃。拖动滑杆可实时预览，右侧输入框可直接填写 0 到 100 的任意整数。0% 为实色，数值越高，背景越通透。毛玻璃会柔化岛后的桌面，展开后的分类名和文件名保持清晰。右键菜单随 Windows 应用主题切换雾白或石墨灰底色。
- **收纳**中的「立即整理」会将个人和公共桌面上的文件、文件夹和快捷方式按规则收入灵动岛，再收起全部桌面图标，包括回收站和此电脑。文件保留在原路径。右键灵动岛或进入「设置 → 收纳」，点「恢复桌面图标」即可重新显示，分类收藏仍可使用。新放到桌面的项目可再次点「立即整理」补收。自动整理开关控制日常拖入的新收藏。
- **快捷栏**支持新建收纳文件夹、选择或更换本地目录、固定文件及调整位置。把图标拖到高亮的组可指定归属，拖到栏内插入线可固定入口。
- **整理规则**支持按扩展名、完整文件名或应用标识匹配，可预览新收纳的桌面项目和已有收藏的分类变化。「保存并整理」会同时收起桌面图标。手动放置优先。

产品范围、三种收纳形态与内存验证目标见[项目设计记录](.agents/notes/module.md)。
