# CompareShow

CompareShow 是一个原生桌面程序，用来并排查看、对比和复制本地目录与 SFTP 目录。窗口不依赖浏览器，可在 macOS、Windows 和 Linux 上编译运行。

左右两侧都可以单独使用。只填一侧时，它是这个位置的文件列表；两侧都填上之后，同名文件会标出相同、不同、只在左侧或只在右侧。

当前版本：**2.0.0**

## 界面

左右对照当前这一层。橙色是内容不同，蓝色只在左侧，紫色只在右侧。

![目录对比](docs/images/folder-compare.png)

双击文本文件后按语法上色。点行中间的箭头，或先选中一段，再复制到另一侧。

![逐行差异](docs/images/line-diff.png)

## 功能

**目录**

- 列出当前这一层。单击文件夹就地展开，再点一次折叠。
- 两种比较方式：**时间 / 大小**，或按文件 **内容**（大小相同的文件计算 MD5）。
- 可用过滤规则跳过指定名字，默认忽略 `node_modules`、`.git`、`.svn`、`dist`、`out`、`.DS_Store`、`Thumbs.db`。
- 记住最近 24 次打开的位置。下次启动会直接打开上一次的位置。

**文本**

- 双击文本文件，左右逐行对照。按扩展名做语法高亮。
- 单击一行，或按住 Shift、拖动，选中一段，再复制到另一侧。也可以点差异行中间的箭头，只复制这一行。
- 复制先改内存中的文本。点「保存左侧」或「保存右侧」后才写回文件。
- 超过 8MB 的文件，以及判定为二进制的文件，只比较哈希，不打开正文。

**文件操作**

- 在列表里把选中项复制到另一侧。复制文件夹时会带上其中的内容。
- 删除文件或文件夹前会确认。当前打开的根目录，以及 `/`，不能删除。
- 查看属性：路径、类型、大小、修改时间、权限。

**SFTP**

- 支持本地对本地、本地对 SFTP、SFTP 对 SFTP。
- 认证顺序：SSH agent、`~/.ssh` 私钥、密码、键盘交互。会读取 `~/.ssh/config` 里的 Host、User、Port、IdentityFile。
- 连接和读取目录时，界面显示当前步骤，可以取消。
- 已知主机同时对照 `~/.ssh/known_hosts` 和 `~/.ssh/compareshow_known_hosts`。密钥不符会拒绝连接。第一次见到的主机只写入 `compareshow_known_hosts`。

## 安装

需要 [Rust](https://www.rust-lang.org/tools/install) stable（1.88 或更新）。首次编译会在本地构建 OpenSSL 和 libssh2，时间会久一些。

```bash
git clone <仓库地址>
cd compareshow
cargo run --release
```

调试构建：

```bash
cargo run
```

## 路径

在左右路径栏输入位置，回车打开。一侧可以留空。

| 写法 | 含义 |
| --- | --- |
| `/Users/me/project`、`~/project`、`C:\work` | 本地目录 |
| `sftp://user@host/var/www` | SFTP，默认端口 22 |
| `sftp://user@host:2222/var/www` | 指定端口 |
| `ssh://user@host/path`、`scp://user@host/path` | 同样按 SFTP 打开 |
| `user@host` | 省略协议时，按远程地址处理 |
| `sftp://user:password@host/path` | 可以用地址里的密码登录；这个密码不会写入会话文件 |

本地目录也可以点「选择」。点「↑」进入上级目录。

## 操作

| 操作 | 说明 |
| --- | --- |
| 回车 | 打开路径栏中的位置 |
| 单击文件夹 | 展开或折叠 |
| 双击文件 | 打开逐行差异 |
| Shift，或拖动 | 在差异视图里选中连续的行 |
| 选中 N 行复制到右 / 左 | 只复制选中的行 |
| 行中间的箭头 | 复制这一行；若这一行属于多行选区，则复制整个选区 |
| 用左侧覆盖右侧 | 整份文件覆盖，与选中行复制不同 |
| ← / → | 在文件列表里，把选中项复制到另一侧 |
| Delete、Backspace | 删除选中项，先确认 |
| F5 | 重新读取；在差异页则重新打开文件 |
| Esc | 关闭差异页、属性或确认框 |

工具栏可以在「时间 / 大小」和「内容」之间切换，也可以刷新、取消、打开过滤和历史。

## 数据放在哪里

会话文件权限为 `0600`（Unix）。里面有比较方式和最近位置，**没有密码**。

| 系统 | 会话文件 |
| --- | --- |
| macOS | `~/Library/Application Support/com.compareshow.CompareShow.CompareShow2/sessions.json` |
| Linux | `~/.local/share/compareshow2/sessions.json` |
| Windows | `%APPDATA%\CompareShow\CompareShow2\data\sessions.json` |

勾选「记住密码」后，密码写入系统钥匙串（服务名 `CompareShow2`）。取消勾选会删掉对应条目。地址里自带的密码只用于这一次登录，不会记住。

## 开发

```bash
cargo test
cargo build --release
```

源码在 `src/`。界面用 egui，SFTP 用 libssh2，目录和文本差异在本机线程里完成。

## 许可证

[MIT](LICENSE)
