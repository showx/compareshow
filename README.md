# CompareShow

本地和 SFTP 文件夹对比工具。界面是原生窗口，支持 macOS、Windows、Linux。

可以只开一侧，也可以左右一起看：**本地 ↔ 本地**、**本地 ↔ SFTP**、**SFTP ↔ SFTP**。

## 功能

- 单击文件夹就地展开，不用双击进入
- 按修改时间和大小比较，或按文件内容比较
- 文本逐行对照，带语法高亮；可以只选中若干行复制到另一侧
- 删除文件或文件夹前会确认，也可以查看路径、大小、时间和权限
- 记住最近打开的位置，下次启动直接打开
- 路径可以是本地目录，或 `sftp://user@host/path`、`ssh://user@host:22/path`
- 优先用 SSH agent 和 `~/.ssh` 私钥，并读取 `~/.ssh/config`
- 新主机密钥写到 `~/.ssh/compareshow_known_hosts`
- 勾选记住密码后，密码放进系统钥匙串，不写进会话文件

默认忽略：`node_modules`、`.git`、`.svn`、`dist`、`out`、`.DS_Store`、`Thumbs.db`。

## 环境

- Rust stable（1.88 或更新）
- 首次编译会在本地构建 OpenSSL 和 libssh2

## 构建与运行

```bash
cargo run --release
```

开发时：

```bash
cargo test
cargo run
```

## 操作

| 操作 | 说明 |
| --- | --- |
| 回车 | 打开路径栏里的位置 |
| 单击文件夹 | 展开或折叠 |
| 双击文件 | 打开逐行差异 |
| Shift 或拖动 | 在差异里选中一段，再点「选中复制到右」 |
| 行中间的箭头 | 把这一行复制到另一侧 |
| Delete | 删除选中项，先确认 |
| F5 | 重新读取；在差异页则重新打开文件 |
| Esc | 关闭差异页或对话框 |

会话保存在系统应用数据目录。macOS 上是 `~/Library/Application Support/com.compareshow.CompareShow2/sessions.json`。密码不在这个文件里。

## 许可证

MIT
