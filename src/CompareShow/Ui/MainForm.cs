using CompareShow.Core;

namespace CompareShow.Ui;

public sealed class MainForm : Form
{
    readonly AppServices _app;
    readonly FlowLayoutPanel _tabs;
    readonly Panel _host;
    readonly ListBox _recent;
    readonly ListBox _connections;
    readonly List<TabItem> _items = [];
    TabItem? _active;

    public MainForm(AppServices app)
    {
        _app = app;
        Text = "CompareShow";
        StartPosition = FormStartPosition.CenterScreen;
        MinimumSize = new Size(1100, 700);
        Size = new Size(1440, 920);
        Theme.StyleForm(this);
        Font = Theme.Ui;
        KeyPreview = true;

        var title = new Panel { Dock = DockStyle.Top, Height = 40, BackColor = Theme.Bg };
        var brand = new Label
        {
            Text = "  CompareShow",
            Font = Theme.UiBold,
            ForeColor = Theme.Muted,
            AutoSize = false,
            Width = 140,
            Height = 40,
            TextAlign = ContentAlignment.MiddleLeft,
            Dock = DockStyle.Left
        };
        _tabs = new FlowLayoutPanel
        {
            Dock = DockStyle.Fill,
            WrapContents = false,
            Padding = new Padding(4, 6, 4, 0)
        };
        var add = Theme.Ghost("+");
        add.Width = 32;
        add.Dock = DockStyle.Right;
        add.Click += (_, _) => AddCompareTab();
        title.Controls.Add(add);
        title.Controls.Add(_tabs);
        title.Controls.Add(brand);

        var sidebar = new Panel { Dock = DockStyle.Left, Width = 230, BackColor = Theme.Raise, Padding = new Padding(10) };
        var newCmp = Theme.Primary("新建比较");
        newCmp.Dock = DockStyle.Top;
        newCmp.Height = 32;
        newCmp.Click += (_, _) => AddCompareTab();
        var recentLab = Theme.Label("最近会话", true);
        recentLab.Dock = DockStyle.Top;
        recentLab.Padding = new Padding(0, 14, 0, 4);
        _recent = DarkList();
        _recent.Dock = DockStyle.Top;
        _recent.Height = 220;
        _recent.DoubleClick += (_, _) => OpenRecent();
        var connHead = new Panel { Dock = DockStyle.Top, Height = 28 };
        var connLab = Theme.Label("SFTP 连接", true);
        connLab.Location = new Point(0, 8);
        var newConn = Theme.Ghost("新建");
        newConn.Location = new Point(150, 2);
        newConn.Click += (_, _) => EditConnection(null);
        connHead.Controls.Add(connLab);
        connHead.Controls.Add(newConn);
        _connections = DarkList();
        _connections.Dock = DockStyle.Fill;
        _connections.DoubleClick += (_, _) =>
        {
            var c = SelectedConnection();
            if (c is not null) EditConnection(c);
        };
        sidebar.Controls.Add(_connections);
        sidebar.Controls.Add(connHead);
        sidebar.Controls.Add(_recent);
        sidebar.Controls.Add(recentLab);
        sidebar.Controls.Add(newCmp);

        _host = new Panel { Dock = DockStyle.Fill, BackColor = Theme.Bg };
        Controls.Add(_host);
        Controls.Add(sidebar);
        Controls.Add(title);

        AddCompareTab();
        ReloadSidebar();
        FormClosing += (_, _) =>
        {
            foreach (var item in _items)
            {
                try { item.Page.Dispose(); } catch { /* ignore */ }
            }
        };
        FormClosed += (_, _) => _app.Sftp.DisconnectAll();
    }

    static ListBox DarkList() => new()
    {
        BackColor = Color.FromArgb(18, 22, 30),
        ForeColor = Theme.Text,
        BorderStyle = BorderStyle.FixedSingle,
        Font = Theme.Small,
        IntegralHeight = false
    };

    void AddCompareTab(LocationRef? left = null, LocationRef? right = null, string? title = null)
    {
        var page = new ComparePage(_app);
        page.Toast += ShowToast;
        page.NewConnectionRequested += () => EditConnection(null);
        page.ConnectionsChanged += ReloadSidebar;
        page.OpenFile += (l, r, name) => AddFileTab(l, r, name);
        page.LocationsChanged += (l, r, t) =>
        {
            if (_active?.Page == page)
            {
                _active.Title = t;
                RenderTabs();
            }
            Remember(l, r, t);
        };
        if (left is not null || right is not null)
            page.SetLocations(left ?? new LocationRef(), right ?? new LocationRef());
        var item = new TabItem { Id = Guid.NewGuid().ToString("N"), Title = title ?? "文件夹比较", Page = page };
        _items.Add(item);
        Activate(item);
    }

    void AddFileTab(LocationRef left, LocationRef right, string name)
    {
        var page = new FileDiffPage(_app, left, right);
        page.Toast += ShowToast;
        var item = new TabItem { Id = Guid.NewGuid().ToString("N"), Title = name, Page = page };
        _items.Add(item);
        Activate(item);
    }

    void Activate(TabItem item)
    {
        _active = item;
        _host.SuspendLayout();
        for (var i = _host.Controls.Count - 1; i >= 0; i--)
        {
            var c = _host.Controls[i];
            _host.Controls.RemoveAt(i);
            c.Hide();
        }
        item.Page.Dock = DockStyle.Fill;
        item.Page.Show();
        _host.Controls.Add(item.Page);
        _host.ResumeLayout();
        RenderTabs();
    }

    void CloseTab(TabItem item)
    {
        _items.Remove(item);
        if (_host.Controls.Contains(item.Page))
            _host.Controls.Remove(item.Page);
        item.Page.Dispose();
        if (_items.Count == 0) AddCompareTab();
        else Activate(_items[^1]);
    }

    void RenderTabs()
    {
        _tabs.SuspendLayout();
        _tabs.Controls.Clear();
        foreach (var item in _items)
        {
            var b = Theme.Ghost(item.Title);
            if (item == _active) b.BackColor = Theme.Active;
            var captured = item;
            b.Click += (_, _) => Activate(captured);
            var close = new Label
            {
                Text = " ×",
                AutoSize = true,
                ForeColor = Theme.Faint,
                Cursor = Cursors.Hand
            };
            close.Click += (_, e) => { CloseTab(captured); };
            var wrap = new FlowLayoutPanel { AutoSize = true, WrapContents = false, BackColor = Color.Transparent, Margin = new Padding(2, 0, 2, 0) };
            wrap.Controls.Add(b);
            if (_items.Count > 1) wrap.Controls.Add(close);
            _tabs.Controls.Add(wrap);
        }
        _tabs.ResumeLayout();
    }

    void ReloadSidebar()
    {
        _recent.Items.Clear();
        foreach (var r in _app.Store.ListRecent())
            _recent.Items.Add(r);
        _recent.DisplayMember = nameof(RecentSession.Title);
        _connections.Items.Clear();
        foreach (var c in _app.Store.ListConnections())
            _connections.Items.Add(c);
        _connections.DisplayMember = nameof(SftpConnectionConfig.Name);
        foreach (var item in _items)
            if (item.Page is ComparePage p) p.ReloadConnections();
    }

    void OpenRecent()
    {
        if (_recent.SelectedItem is not RecentSession r) return;
        AddCompareTab(
            new LocationRef { Kind = r.LeftKind, Path = r.LeftPath, ConnectionId = r.LeftConnectionId },
            new LocationRef { Kind = r.RightKind, Path = r.RightPath, ConnectionId = r.RightConnectionId },
            r.Title);
    }

    SftpConnectionConfig? SelectedConnection() => _connections.SelectedItem as SftpConnectionConfig;

    void EditConnection(SftpConnectionConfig? initial)
    {
        var full = initial is null ? null : _app.Store.GetConnection(initial.Id) ?? initial;
        using var dlg = new ConnectForm(_app, full);
        if (dlg.ShowDialog(this) == DialogResult.OK) ReloadSidebar();
    }

    void Remember(LocationRef left, LocationRef right, string title)
    {
        if (string.IsNullOrEmpty(left.Path) || string.IsNullOrEmpty(right.Path)) return;
        _app.Store.AddRecent(new RecentSession
        {
            Id = Guid.NewGuid().ToString("N"),
            Title = title,
            LeftKind = left.Kind,
            RightKind = right.Kind,
            LeftPath = left.Path,
            RightPath = right.Path,
            LeftConnectionId = left.ConnectionId,
            RightConnectionId = right.ConnectionId,
            At = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds()
        });
        ReloadSidebar();
    }

    void ShowToast(string text, bool error)
    {
        var strip = Controls.OfType<Panel>().FirstOrDefault(p => p.Name == "toast");
        strip?.Dispose();
        var p = new Panel
        {
            Name = "toast",
            Height = 32,
            Dock = DockStyle.Bottom,
            BackColor = error ? Color.FromArgb(80, 30, 30) : Color.FromArgb(24, 64, 48)
        };
        var lab = new Label { Text = text, Dock = DockStyle.Fill, ForeColor = Theme.Text, TextAlign = ContentAlignment.MiddleLeft, Padding = new Padding(12, 0, 12, 0) };
        p.Controls.Add(lab);
        Controls.Add(p);
        p.BringToFront();
        var t = new System.Windows.Forms.Timer { Interval = 3200 };
        t.Tick += (_, _) => { t.Stop(); t.Dispose(); p.Dispose(); };
        t.Start();
    }

    protected override bool ProcessCmdKey(ref Message msg, Keys keyData)
    {
        if (keyData == Keys.F5 && _active?.Page is ComparePage page)
        {
            _ = page.RefreshAsync();
            return true;
        }
        return base.ProcessCmdKey(ref msg, keyData);
    }

    sealed class TabItem
    {
        public required string Id { get; init; }
        public string Title { get; set; } = "";
        public required UserControl Page { get; init; }
    }
}
