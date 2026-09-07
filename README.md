# CompareShow

面向本地与 SFTP 的桌面文件对比工具。并排查看目录差异、逐行比对文本、把变更同步到另一侧，交互接近 Beyond Compare。

支持 **本地 ↔ 本地**、**本地 ↔ SFTP**、**SFTP ↔ SFTP**。

## 功能

- 文件夹树状对比：相同 / 不同 / 仅左 / 仅右
- 两种比较规则
  - **按时间/大小**：快速扫描
  - **按文件内容**：MD5，更准、更慢
- 文本文件并排 / 内联差异（Monaco Editor），可跳转到上一处 / 下一处
- 二进制或超过 8MB 的文件只比哈希，不打开文本
- 复制到左 / 右、同步差异、删除选中项
- 路径栏直接输入地址回车即可列出文件
  - 本地路径
  - `ssh://root@主机`
  - `sftp://user@host:22/path`
  - `user@host`
- SFTP 优先使用本机 `~/.ssh` 私钥和 ssh-agent，密钥失败后再提示密码
- 读取 `~/.ssh/config`（Host、User、Port、IdentityFile）
- 保存连接与最近会话；密码使用系统安全存储加密

## 环境要求

- Node.js 18+
- npm
- Windows / macOS / Linux（开发与运行均基于 Electron）

## 快速开始

```bash
npm install
npm run dev
```

首次启动后，在左右路径栏分别填入本地目录或 SFTP 地址，回车列出文件，再点 **比较**。

也可以在侧栏 **新建 SFTP 连接**，保存后从路径栏下拉选用。

## 常用操作

| 操作 | 说明 |
| --- | --- |
| 回车 | 解析路径并列出当前目录 |
| 双击文件 | 打开文本差异 |
| 双击文件夹 | 进入该目录 |
| 三角按钮 | 展开 / 折叠子树 |
| Ctrl / Shift 点击 | 多选 |
| Backspace | 返回上级 |
| → / ← | 把当前项复制到对侧 |

默认忽略：`node_modules`、`.git`、`.svn`、`dist`、`out`、`.DS_Store`、`Thumbs.db`。

## 脚本

| 命令 | 作用 |
| --- | --- |
| `npm run dev` | 开发模式启动 |
| `npm run build` | 编译主进程、预加载与渲染进程到 `out/` |
| `npm run preview` | 预览已构建的应用 |

## 项目结构

```
src/
  main/          Electron 主进程：比较、SFTP、本地文件系统、传输
  preload/       安全 IPC 桥
  renderer/      React 界面
  shared/        共享类型与 SFTP URL 解析
```

连接与最近会话保存在 Electron `userData` 下的 `sessions.json`。密码、私钥口令会加密存储，界面里不会回显明文。

## 技术栈

Electron · React 19 · TypeScript · electron-vite · Monaco Editor · ssh2

## 许可证

MIT
