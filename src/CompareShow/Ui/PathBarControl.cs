using CompareShow.Core;

namespace CompareShow.Ui;

public sealed class PathBarControl : UserControl
{
    readonly AppServices _app;
    readonly ComboBox _source;
    readonly TextBox _path;
    readonly Button _up;
    readonly Button _primary;
    readonly Button _browse;
    readonly Label _status;
    List<SftpConnectionConfig> _connections = [];
    LocationRef _value = new();
    bool _updating;

    public event Action<LocationRef>? LocationCommitted;
    public event Action? NewConnectionRequested;
    public event Action? ConnectionsChanged;
    public event Action<string, bool>? Toast;
    public event Action? Submitted;

    public PathBarControl(AppServices app, string sideLabel)
    {
        _app = app;
        Height = 58;
        Dock = DockStyle.Top;
        BackColor = Theme.Bg;

        _source = Theme.Combo();
        _source.Width = 110;
        _source.Anchor = AnchorStyles.Left | AnchorStyles.Top;
        _source.SelectedIndexChanged += (_, _) => { if (!_updating) OnSource(); };

        _path = Theme.Input();
        _path.Anchor = AnchorStyles.Left | AnchorStyles.Right | AnchorStyles.Top;
        _path.KeyDown += async (_, e) =>
        {
            if (e.KeyCode == Keys.Enter)
            {
                e.SuppressKeyPress = true;
                await SubmitAsync();
            }
        };

        _up = Theme.Ghost("↑");
        _up.Width = 32;
        _up.Anchor = AnchorStyles.Right | AnchorStyles.Top;
        _up.Click += (_, _) =>
        {
            LocationCommitted?.Invoke(LocationPaths.ParentPath(_value));
        };

        _primary = Theme.Primary("列出");
        _primary.Anchor = AnchorStyles.Right | AnchorStyles.Top;
        _primary.Click += async (_, _) => await SubmitAsync();

        _browse = Theme.Ghost("浏览");
        _browse.Anchor = AnchorStyles.Right | AnchorStyles.Top;
        _browse.Click += async (_, _) => await BrowseAsync();

        _status = new Label
        {
            ForeColor = Theme.Muted,
            AutoSize = false,
            Height = 18,
            Dock = DockStyle.Bottom,
            TextAlign = ContentAlignment.MiddleLeft
        };

        var row = new Panel { Dock = DockStyle.Top, Height = 34, Padding = new Padding(0, 4, 0, 0) };
        row.Controls.AddRange(new Control[] { _source, _path, _up, _primary, _browse });
        Controls.Add(_status);
        Controls.Add(row);
        Resize += (_, _) => LayoutRow();
        LayoutRow();
        _ = sideLabel;
    }

    void LayoutRow()
    {
        const int gap = 6;
        _source.Location = new Point(0, 2);
        _source.Height = 26;
        _browse.Location = new Point(Width - _browse.Width, 1);
        _primary.Location = new Point(_browse.Left - gap - _primary.Width, 1);
        _up.Location = new Point(_primary.Left - gap - _up.Width, 1);
        _path.Location = new Point(_source.Right + gap, 3);
        _path.Width = Math.Max(40, _up.Left - gap - _path.Left);
        _status.Padding = new Padding(0, 0, 0, 0);
    }

    public void Bind(LocationRef value, IReadOnlyList<SftpConnectionConfig> connections)
    {
        _value = value.Clone();
        _connections = connections.ToList();
        _updating = true;
        _source.Items.Clear();
        _source.Items.Add("本地");
        foreach (var c in _connections) _source.Items.Add(c.Name);
        _source.Items.Add("＋ 新建 SFTP…");
        _source.SelectedIndex = value.Kind == LocationKind.Local
            ? 0
            : Math.Max(0, _connections.FindIndex(c => c.Id == value.ConnectionId) + 1);
        if (_source.SelectedIndex < 0) _source.SelectedIndex = 0;
        _path.Text = LocationPaths.LocationDisplay(value, _connections);
        UpdateButtons();
        _updating = false;
    }

    public LocationRef Current => _value.Clone();
    public string Draft => _path.Text;

    void UpdateButtons()
    {
        var remote = SftpUrl.LooksLikeSftpInput(_path.Text) || _value.Kind == LocationKind.Sftp;
        _primary.Text = remote && !LocationPaths.LooksLikeLocalPath(_path.Text) ? "连接" : "列出";
    }

    void OnSource()
    {
        if (_source.SelectedIndex < 0) return;
        if (_source.SelectedIndex == _source.Items.Count - 1)
        {
            NewConnectionRequested?.Invoke();
            return;
        }
        if (_source.SelectedIndex == 0)
        {
            LocationCommitted?.Invoke(new LocationRef { Kind = LocationKind.Local, Path = _value.Kind == LocationKind.Local ? _value.Path : "" });
            return;
        }
        var c = _connections[_source.SelectedIndex - 1];
        LocationCommitted?.Invoke(new LocationRef
        {
            Kind = LocationKind.Sftp,
            ConnectionId = c.Id,
            Path = string.IsNullOrEmpty(c.DefaultPath) ? "/" : c.DefaultPath
        });
    }

    public async Task SubmitAsync()
    {
        var text = _path.Text.Trim();
        if (text.Length == 0)
        {
            if (_value.Kind == LocationKind.Sftp && !string.IsNullOrEmpty(_value.ConnectionId))
            {
                var root = _value.Clone();
                root.Path = "/";
                LocationCommitted?.Invoke(root);
                Submitted?.Invoke();
                return;
            }
            ApplyLocal(text.Length == 0 ? "\\" : text);
            Submitted?.Invoke();
            return;
        }
        if (LocationPaths.LooksLikeLocalPath(text))
        {
            ApplyLocal(text);
            Submitted?.Invoke();
            return;
        }
        if (SftpUrl.LooksLikeSftpInput(text))
        {
            var parsed = SftpUrl.Parse(text);
            if (parsed is null)
            {
                Toast?.Invoke("无法解析地址，请用 sftp://root@主机", true);
                return;
            }
            await OpenRemoteAsync(text, parsed.Password);
            Submitted?.Invoke();
            return;
        }
        if (_value.Kind == LocationKind.Sftp && !string.IsNullOrEmpty(_value.ConnectionId))
        {
            var path = text.StartsWith('/') ? text : "/" + text;
            path = System.Text.RegularExpressions.Regex.Replace(path, "/{2,}", "/");
            if (path.Length == 0) path = "/";
            var next = _value.Clone();
            next.Path = path;
            LocationCommitted?.Invoke(next);
            Submitted?.Invoke();
            return;
        }
        ApplyLocal(text);
        Submitted?.Invoke();
    }

    void ApplyLocal(string text)
    {
        var path = LocationPaths.NormalizeLocalInput(text);
        if (path.Length == 0) path = "\\";
        _status.Text = "";
        try
        {
            var resolved = LocalFs.Resolve(text.Length == 0 ? path : text);
            path = resolved.Path;
            if (resolved.Type == "file") Toast?.Invoke("已打开文件所在目录", false);
        }
        catch { /* 列表时再报错 */ }
        LocationCommitted?.Invoke(new LocationRef { Kind = LocationKind.Local, Path = path });
    }

    async Task OpenRemoteAsync(string url, string? password)
    {
        _primary.Enabled = false;
        _status.ForeColor = Theme.Muted;
        _status.Text = "正在用本机 SSH 密钥连接…";
        try
        {
            var res = await _app.OpenUrlAsync(url, password, true);
            if (res.Ok)
            {
                _status.ForeColor = Theme.Ok;
                _status.Text = "已连接";
                ConnectionsChanged?.Invoke();
                LocationCommitted?.Invoke(new LocationRef
                {
                    Kind = LocationKind.Sftp,
                    ConnectionId = res.ConnectionId,
                    Path = res.Path ?? "/",
                    DisplayUrl = res.DisplayUrl
                });
                return;
            }
            if (res.NeedPassword)
            {
                _status.Text = "";
                using var dlg = new PasswordForm(
                    $"连接 {res.Username}@{res.Host}",
                    res.Error ?? "密钥登录未成功。密钥来自本机 %USERPROFILE%\\.ssh。");
                if (dlg.ShowDialog(FindForm()) != DialogResult.OK || string.IsNullOrEmpty(dlg.Password))
                    return;
                var again = await _app.OpenUrlAsync(url, dlg.Password, dlg.Remember);
                if (again.Ok)
                {
                    _status.ForeColor = Theme.Ok;
                    _status.Text = "已连接";
                    ConnectionsChanged?.Invoke();
                    LocationCommitted?.Invoke(new LocationRef
                    {
                        Kind = LocationKind.Sftp,
                        ConnectionId = again.ConnectionId,
                        Path = again.Path ?? "/",
                        DisplayUrl = again.DisplayUrl
                    });
                    return;
                }
                _status.ForeColor = Theme.Danger;
                _status.Text = again.Error ?? "连接失败";
                Toast?.Invoke(_status.Text, true);
                return;
            }
            _status.ForeColor = Theme.Danger;
            _status.Text = res.Error ?? "连接失败";
            Toast?.Invoke(_status.Text, true);
        }
        finally { _primary.Enabled = true; UpdateButtons(); }
    }

    async Task BrowseAsync()
    {
        if (_value.Kind == LocationKind.Sftp && !string.IsNullOrEmpty(_value.ConnectionId))
        {
            using var dlg = new RemoteBrowserForm(_app, _value.ConnectionId, string.IsNullOrEmpty(_value.Path) ? "/" : _value.Path);
            if (dlg.ShowDialog(FindForm()) == DialogResult.OK)
            {
                var next = _value.Clone();
                next.Path = dlg.SelectedPath;
                next.DisplayUrl = null;
                LocationCommitted?.Invoke(next);
            }
            return;
        }
        using var fd = new FolderBrowserDialog { Description = "选择文件夹", UseDescriptionForTitle = true };
        if (fd.ShowDialog(FindForm()) == DialogResult.OK)
            ApplyLocal(fd.SelectedPath);
        await Task.CompletedTask;
    }
}
