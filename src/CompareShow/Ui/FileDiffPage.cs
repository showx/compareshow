using CompareShow.Core;

namespace CompareShow.Ui;

public sealed class FileDiffPage : UserControl
{
    readonly AppServices _app;
    readonly LocationRef _left;
    readonly LocationRef _right;
    readonly Label _meta;
    readonly Label _status;
    readonly Panel _host;
    readonly Button _prev;
    readonly Button _next;
    readonly Button _copyLineR;
    readonly Button _copyLineL;
    readonly Button _saveLeft;
    readonly Button _saveRight;
    readonly Button _ws;
    readonly LineDiffControl _diff = new() { Dock = DockStyle.Fill };
    bool _ignoreWs;
    FileCompareResult? _data;
    bool _binary;

    public event Action<string, bool>? Toast;

    public FileDiffPage(AppServices app, LocationRef left, LocationRef right)
    {
        _app = app;
        _left = left;
        _right = right;
        Dock = DockStyle.Fill;
        BackColor = Theme.Bg;
        Font = Theme.Ui;

        var toolbar = new FlowLayoutPanel
        {
            Dock = DockStyle.Top,
            Height = 72,
            Padding = new Padding(8, 6, 8, 4),
            WrapContents = true
        };
        _status = Theme.Label("正在读取文件…", true);
        _prev = Theme.Ghost("上一处");
        _next = Theme.Ghost("下一处");
        _copyLineR = Theme.Ghost("本行 → 右");
        _copyLineL = Theme.Ghost("本行 ← 左");
        var hunkR = Theme.Ghost("差异块 → 右");
        var hunkL = Theme.Ghost("差异块 ← 左");
        _saveLeft = Theme.Ghost("保存左");
        _saveRight = Theme.Ghost("保存右");
        _ws = Theme.Ghost("忽略空白");
        _prev.Click += (_, _) => _diff.Jump(-1);
        _next.Click += (_, _) => _diff.Jump(1);
        _copyLineR.Click += (_, _) => _diff.CopyLine(true);
        _copyLineL.Click += (_, _) => _diff.CopyLine(false);
        hunkR.Click += (_, _) => _diff.CopyHunk(true);
        hunkL.Click += (_, _) => _diff.CopyHunk(false);
        var realign = Theme.Ghost("重新对齐");
        realign.Click += (_, _) => _diff.Realign(_ignoreWs);
        _saveLeft.Click += async (_, _) => await SaveAsync(true);
        _saveRight.Click += async (_, _) => await SaveAsync(false);
        _ws.Click += (_, _) =>
        {
            if (_diff.DirtyLeft || _diff.DirtyRight)
            {
                if (MessageBox.Show(this, "重新对比会丢掉未保存的行内修改，继续？", "忽略空白",
                        MessageBoxButtons.OKCancel, MessageBoxIcon.Question) != DialogResult.OK)
                    return;
            }
            _ignoreWs = !_ignoreWs;
            _ws.BackColor = _ignoreWs ? Theme.Active : Theme.Raise;
            RenderText();
        };
        var copyFileR = Theme.Ghost("整文件 → 右");
        copyFileR.Click += async (_, _) => await CopyFileAsync(true);
        var copyFileL = Theme.Ghost("整文件 ← 左");
        copyFileL.Click += async (_, _) => await CopyFileAsync(false);
        toolbar.Controls.AddRange(new Control[]
        {
            _status, _prev, _next, _copyLineR, _copyLineL, hunkR, hunkL,
            realign, _saveLeft, _saveRight, _ws, copyFileR, copyFileL
        });

        _meta = new Label
        {
            Dock = DockStyle.Top,
            Height = 44,
            ForeColor = Theme.Muted,
            Padding = new Padding(8, 4, 8, 4)
        };
        var hint = new Label
        {
            Dock = DockStyle.Top,
            Height = 22,
            ForeColor = Theme.Faint,
            Padding = new Padding(8, 0, 8, 0),
            Text = "单击左右文本即可编辑 · Enter 换行 · Backspace 合并上行 · Ctrl+S 保存 · 改完后可点「重新对齐」"
        };
        _host = new Panel { Dock = DockStyle.Fill, BackColor = Theme.Bg, Padding = new Padding(4, 0, 4, 4) };
        _diff.Changed += UpdateStatus;
        _host.Controls.Add(_diff);

        Controls.Add(_host);
        Controls.Add(hint);
        Controls.Add(_meta);
        Controls.Add(toolbar);
        Load += async (_, _) => await LoadAsync();
    }

    async Task LoadAsync()
    {
        try
        {
            await _app.EnsureConnectedAsync(_left);
            await _app.EnsureConnectedAsync(_right);
            if (IsDisposed) return;
            _data = await _app.Compare.CompareFileAsync(_left, _right, CancellationToken.None);
            if (IsDisposed) return;
            _meta.Text = $"{SideTitle("左", _left)}{FormatMetaSize(_data.LeftMeta)}\r\n{SideTitle("右", _right)}{FormatMetaSize(_data.RightMeta)}";
            Render();
        }
        catch (ObjectDisposedException) { }
        catch (Exception ex)
        {
            if (IsDisposed) return;
            _status.Text = ex.Message;
            Toast?.Invoke(ex.Message, true);
        }
    }

    static string SideTitle(string side, LocationRef loc)
        => string.IsNullOrEmpty(loc.Path) ? $"{side}：（不存在）" : $"{side}：{loc.Path}";

    static string FormatMetaSize(FileMeta? m) => m is null ? "" : $"  ·  {TextUtil.FormatSize(m.Size)}";

    void Render()
    {
        if (_data is null) return;
        _binary = _data.LeftBinary || _data.RightBinary || _data.TooLarge;
        if (_binary)
        {
            _status.Text = _data.Identical ? "内容相同" : _data.TooLarge ? "文件过大，已跳过全文加载" : "二进制或超大文件";
            _prev.Enabled = _next.Enabled = _copyLineL.Enabled = _copyLineR.Enabled = _ws.Enabled = false;
            _saveLeft.Enabled = _saveRight.Enabled = false;
            _diff.Hide();
            var existing = _host.Controls.OfType<Label>().FirstOrDefault();
            if (existing is null)
            {
                existing = new Label
                {
                    Dock = DockStyle.Fill,
                    ForeColor = Theme.Text,
                    Padding = new Padding(24)
                };
                _host.Controls.Add(existing);
            }
            existing.Text = $"{_status.Text}\r\n\r\n左侧 MD5：{_data.LeftHash ?? "—"}\r\n右侧 MD5：{_data.RightHash ?? "—"}\r\n\r\n可用「整文件」按钮整份覆盖。";
            existing.Show();
            existing.BringToFront();
            return;
        }
        foreach (Control c in _host.Controls)
            if (c is Label) c.Hide();
        _diff.Show();
        _diff.BringToFront();
        RenderText();
    }

    void RenderText()
    {
        if (_data is null || _binary) return;
        var left = _data.LeftText ?? "";
        var right = _data.RightText ?? "";
        _diff.LoadTexts(left, right, _ignoreWs);
        UpdateStatus();
    }

    void UpdateStatus()
    {
        if (IsDisposed || _binary) return;
        var n = _diff.DiffCount;
        var dirty = (_diff.DirtyLeft ? " · 左侧已改" : "") + (_diff.DirtyRight ? " · 右侧已改" : "");
        _status.Text = n == 0 ? "内容相同" + dirty : $"文本差异 {n} 处" + dirty;
        _prev.Enabled = _next.Enabled = n > 0;
        _copyLineL.Enabled = _copyLineR.Enabled = true;
        _saveLeft.Enabled = !string.IsNullOrEmpty(_left.Path);
        _saveRight.Enabled = !string.IsNullOrEmpty(_right.Path);
    }

    async Task SaveAsync(bool left)
    {
        var loc = left ? _left : _right;
        if (string.IsNullOrEmpty(loc.Path))
        {
            Toast?.Invoke(left ? "左侧没有可保存的路径" : "右侧没有可保存的路径", true);
            return;
        }
        try
        {
            await _app.EnsureConnectedAsync(loc);
            if (IsDisposed) return;
            var text = left ? _diff.BuildLeft() : _diff.BuildRight();
            await _app.Transfer.WriteTextAsync(loc, text, _data?.Encoding);
            if (IsDisposed) return;
            if (left)
            {
                if (_data is not null) _data.LeftText = text;
                _diff.ClearDirty(true, false);
            }
            else
            {
                if (_data is not null) _data.RightText = text;
                _diff.ClearDirty(false, true);
            }
            Toast?.Invoke(left ? "左侧已保存" : "右侧已保存", false);
        }
        catch (ObjectDisposedException) { }
        catch (Exception ex) { Toast?.Invoke(ex.Message, true); }
    }

    async Task CopyFileAsync(bool toRight)
    {
        var source = toRight ? _left : _right;
        var dest = toRight ? _right : _left;
        if (string.IsNullOrEmpty(source.Path) || string.IsNullOrEmpty(dest.Path))
        {
            Toast?.Invoke("这一侧文件不存在", true);
            return;
        }
        if (_diff.DirtyLeft || _diff.DirtyRight)
        {
            if (MessageBox.Show(this, "整文件覆盖会丢掉未保存的行内修改，继续？", "覆盖",
                    MessageBoxButtons.OKCancel, MessageBoxIcon.Question) != DialogResult.OK)
                return;
        }
        try
        {
            await _app.EnsureConnectedAsync(source);
            await _app.EnsureConnectedAsync(dest);
            await _app.Transfer.CopyAsync(source, LocationPaths.ParentPath(dest), false);
            if (IsDisposed) return;
            Toast?.Invoke(toRight ? "已覆盖到右侧" : "已覆盖到左侧", false);
            await LoadAsync();
        }
        catch (ObjectDisposedException) { }
        catch (Exception ex) { Toast?.Invoke(ex.Message, true); }
    }

    protected override bool ProcessCmdKey(ref Message msg, Keys keyData)
    {
        if (keyData == (Keys.Control | Keys.S))
        {
            if (_diff.DirtyLeft) _ = SaveAsync(true);
            if (_diff.DirtyRight) _ = SaveAsync(false);
            if (!_diff.DirtyLeft && !_diff.DirtyRight)
                Toast?.Invoke("没有要保存的修改", false);
            return true;
        }
        return base.ProcessCmdKey(ref msg, keyData);
    }

    protected override void Dispose(bool disposing)
    {
        if (disposing)
        {
            try { _diff.Changed -= UpdateStatus; } catch { /* ignore */ }
        }
        base.Dispose(disposing);
    }
}
