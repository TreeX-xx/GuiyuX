<!-- wfx-managed: workflowx-entry -->
# WorkflowX

Main Agent owns direct execution, document maintenance and scheduling. User scope and authorization persist; preserve unrelated edits. Parallel Agents require explicit request.

- Use .codex/skills/orchestrateX/SKILL.md for xdo/xdel/xflow/xarch/xstatus. Read only the selected mode's references.
- Ordinary xdo requires and creates no Task. An explicitly selected existing Task retains acceptance/review obligations. Main Agent alone updates the shared Task before every handoff; all Task content is read-only to subagents.
- Use noteX when engineering work changes recorded facts or lasting reasoning. Maintain module directories and stable topic files; no forced Note update for no-op documentation changes.
- Delegated implementation/review uses orchestrateX modules 02 and 09. Never role-play an unavailable independent subagent.
- Formal format: the repository's pinned harness-note/2 profile. Skills/templates are usable without a desktop; use a supported shared wiki reader or bounded rg over original Notes. Report unsupported profiles and unresolved references.
- Agent runtime definitions live in .codex/agents/. Keep local model, approval and unrelated configuration intact.

If source text cannot be read/edited normally, inspect with rg and make precise encoding-preserving edits; verify the diff. Never replace unreadable content blindly.
<!-- wfx-managed-end -->

## Windows 界面验证

用 `./build.ps1 -VerifyOnly` 验证实际窗口。新增界面用例在独立的 Win32 桌面和临时配置中运行，用发给测试窗口的消息或 WebView 的页面接口模拟操作。需要核对画面时从测试窗口离屏取图，用户的光标与前台窗口由用户控制。
