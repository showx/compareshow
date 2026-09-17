# CompareShow

面向本地与 SFTP 的 **Windows 窗体** 文件对比工具。并排查看目录差异、逐行比对文本、把变更同步到另一侧。

支持 **本地 ↔ 本地**、**本地 ↔ SFTP**、**SFTP ↔ SFTP**。

不再使用 Electron / JavaScript。比较、列表、SFTP 传输都在 .NET 原生线程里跑，界面用虚拟绘制列表，大目录时更稳、更好控速。

## 功能

- 文件夹树状对比：相同 / 不同 / 仅左 / 仅右
- 两种比较规则
  - **按时间/大小**：快速扫描
  - **按文件内容**：MD5，更准、更慢
- 文本文件按行并排对比：单击即可编辑全文（换行/删行），复制本行或差异块，分别保存左右两侧
- 二进制或超过 8MB 的文件只比哈希，不打开文本
- 复制到左 / 右、同步差异
- 路径栏直接输入地址回车即可列出文件
  - 本地路径
  - `ssh://root@主机`
  - `sftp://user@host:22/path`
  - `user@host`
- SFTP 优先使用本机 `~/.ssh` 私钥，密钥失败后再提示密码
- 读取 `~/.ssh/config`（Host、User、Port、IdentityFile）
- 保存连接与最近会话；密码使用 Windows DPAPI 加密

## 环境要求

- Windows 10 / 11
- [.NET 8 SDK](https://dotnet.microsoft.com/download/dotnet/8.0)（开发）
- 运行已编译程序只需 .NET 8 Desktop Runtime

## 快速开始

```powershell
dotnet build src/CompareShow/CompareShow.csproj -c Release
dotnet run --project src/CompareShow/CompareShow.csproj -c Release
```

生成结果在：

`src/CompareShow/bin/Release/net8.0-windows/CompareShow.exe`

首次启动后，在左右路径栏分别填入本地目录或 SFTP 地址，回车列出文件，再点 **比较**。

也可以在侧栏 **新建** SFTP 连接，保存后从路径栏下拉选用。

## 常用操作

| 操作 | 说明 |
| --- | --- |
| 回车 | 解析路径并列出当前目录 |
| 双击文件 | 打开文本差异；单击即可编辑，Ctrl+S 保存左/右 |
| 双击文件夹 | 展开 / 折叠（Ctrl+Enter 进入） |
| 三角按钮 / 展开 | 展开 / 折叠子树 |
| Ctrl / Shift 点击 | 多选 |
| Backspace | 返回上级 |
| → / ← | 把当前项复制到对侧 |
| F5 | 重新列出或重新比较 |

默认忽略：`node_modules`、`.git`、`.svn`、`dist`、`out`、`.DS_Store`、`Thumbs.db`。

## 项目结构

```
src/CompareShow/
  Core/     比较引擎、本地文件系统、SFTP、会话存储
  Ui/       WinForms 主窗体、双栏列表、路径栏、文本 Diff
```

连接与最近会话保存在 `%AppData%\CompareShow\sessions.json`。密码、私钥口令使用当前 Windows 用户 DPAPI 加密，界面里不会回显明文。

## 技术栈

C# · .NET 8 WinForms · SSH.NET · DiffPlex

## 许可证

MIT
