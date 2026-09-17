using CompareShow.Core;

namespace CompareShow.Ui;

public sealed class ComparePage : UserControl
{
    public enum ViewFilter { All, Diff, Left, Right, Same }

    readonly AppServices _app;
    readonly PathBarControl _leftBar;
    readonly PathBarControl _rightBar;
    readonly CompareListControl _list;
    readonly ComboBox _mode;
    readonly TextBox _search;
    readonly Label _status;
    readonly Label _hint;
    readonly Button _listBtn;
    readonly Button _cmpBtn;
    readonly Button _copyRight;
    readonly Button _copyLeft;
    readonly Button _syncRight;
    readonly Button _syncLeft;
    readonly FlowLayoutPanel _filters;
    LocationRef _left = new() { Kind = LocationKind.Local };
    LocationRef _right = new() { Kind = LocationKind.Local };
    FolderCompareResult? _result;
    bool _deep;
    bool _busy;
    CancellationTokenSource? _cts;
    ViewFilter _filter = ViewFilter.All;
    string _query = "";
    CompareMode _cmpMode = CompareMode.Quick;

    public event Action<string, bool>? Toast;
    public event Action? NewConnectionRequested;
    public event Action? ConnectionsChanged;
    public event Action<LocationRef, LocationRef, string>? OpenFile;
    public event Action<LocationRef, LocationRef, string>? LocationsChanged;

    public LocationRef LeftLoc => _left;
    public LocationRef RightLoc => _right;

    public ComparePage(AppServices app)
    {
        _app = app;
        Dock = DockStyle.Fill;
        BackColor = Theme.Bg;
        Font = Theme.Ui;

        var toolbar = new FlowLayoutPanel
        {
            Dock = DockStyle.Top,
            Height = 40,
            Padding = new Padding(8, 6, 8, 4),
            WrapContents = false
        };
        _listBtn = Theme.Ghost("列出当前");
        _cmpBtn = Theme.Primary("比较");
        _cmpBtn.Click += async (_, _) => await RunCompareAsync();
        _copyRight = Theme.Ghost("复制到右 →");
        _copyRight.Click += async (_, _) => await CopySelectedAsync(true);
        _copyLeft = Theme.Ghost("← 复制到左");
        _copyLeft.Click += async (_, _) => await CopySelectedAsync(false);
        _syncRight = Theme.Ghost("同步差异到右");
        _syncRight.Click += async (_, _) => await SyncAsync(true);
        _syncLeft = Theme.Ghost("同步差异到左");
        _syncLeft.Click += async (_, _) => await SyncAsync(false);
        var expand = Theme.Ghost("展开");
        var collapse = Theme.Ghost("折叠");
        _mode = Theme.Combo();
        _mode.Items.AddRange(new object[] { "按时间/大小", "按文件内容" });
        _mode.SelectedIndex = 0;
        _mode.Width = 120;
        _mode.SelectedIndexChanged += async (_, _) =>
        {
            _cmpMode = _mode.SelectedIndex == 1 ? CompareMode.Content : CompareMode.Quick;
            if (_deep) await RunCompareAsync();
        };
        _search = Theme.Input();
        _search.PlaceholderText = "过滤文件名";
        _search.Width = 180;
        _search.TextChanged += (_, _) => { _query = _search.Text.Trim(); RebuildRows(); };
        toolbar.Controls.AddRange(new Control[] { _listBtn, _cmpBtn, _copyRight, _copyLeft, _syncRight, _syncLeft, expand, collapse, _mode, _search });

        var paths = new TableLayoutPanel
        {
            Dock = DockStyle.Top,
            Height = 64,
            ColumnCount = 3,
            Padding = new Padding(8, 0, 8, 0)
        };
        paths.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 50));
        paths.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 44));
        paths.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 50));
        _leftBar = new PathBarControl(app, "左");
        _rightBar = new PathBarControl(app, "右");
        var swap = Theme.Ghost("⇄");
        swap.Width = 36;
        swap.Click += (_, _) => Commit(_right.Clone(), _left.Clone());
        paths.Controls.Add(_leftBar, 0, 0);
        paths.Controls.Add(swap, 1, 0);
        paths.Controls.Add(_rightBar, 2, 0);
        _listBtn.Click += async (_, _) =>
        {
            await _leftBar.SubmitAsync();
            await _rightBar.SubmitAsync();
        };

        _filters = new FlowLayoutPanel
        {
            Dock = DockStyle.Top,
            Height = 32,
            Padding = new Padding(8, 2, 8, 0),
            WrapContents = false
        };
        foreach (var (id, text) in new (ViewFilter, string)[]
        {
            (ViewFilter.All, "全部"),
            (ViewFilter.Diff, "差异"),
            (ViewFilter.Left, "仅左"),
            (ViewFilter.Right, "仅右"),
            (ViewFilter.Same, "相同")
        })
        {
            var cap = text;
            var fid = id;
            var b = Theme.Ghost(cap);
            b.Tag = fid;
            b.Click += (_, _) => { _filter = fid; HighlightFilters(); RebuildRows(); };
            _filters.Controls.Add(b);
        }
        _hint = Theme.Label("点「比较」扫描整棵树 · 双击文件夹展开/折叠 · 双击文件按行对比/改保存", true);
        _hint.Padding = new Padding(12, 6, 0, 0);
        _filters.Controls.Add(_hint);

        var header = new Panel { Dock = DockStyle.Top, Height = 24, BackColor = Theme.Raise, Padding = new Padding(8, 0, 24, 0) };
        header.Paint += (_, e) =>
        {
            var w = Math.Max(1, header.Width - 24);
            var mid = Math.Max(1, w / 2);
            var colW = Math.Max(1, mid - 36);
            TextRenderer.DrawText(e.Graphics, "名称                  大小          修改时间", Theme.Small, new Rectangle(8, 0, colW, 24), Theme.Muted, TextFormatFlags.VerticalCenter | TextFormatFlags.EndEllipsis);
            TextRenderer.DrawText(e.Graphics, "名称                  大小          修改时间", Theme.Small, new Rectangle(mid + 36, 0, colW, 24), Theme.Muted, TextFormatFlags.VerticalCenter | TextFormatFlags.EndEllipsis);
        };

        _list = new CompareListControl { Dock = DockStyle.Fill };
        _list.NodeActivated += n => OpenNode(n);
        _list.DirectoryEntered += EnterDir;
        _list.CopyRequested += async (n, toRight) => await CopyNodesAsync([n], toRight);
        _list.GoUpRequested += GoUp;
        _list.SelectionChanged += UpdateButtons;
        _list.ExpandToggled += async n => await OnExpandToggledAsync(n);
        expand.Click += async (_, _) => await ExpandAllAsync();
        collapse.Click += (_, _) => { _list.ClearExpanded(); RebuildRows(true); };

        _status = new Label
        {
            Dock = DockStyle.Bottom,
            Height = 26,
            BackColor = Theme.Raise,
            ForeColor = Theme.Muted,
            TextAlign = ContentAlignment.MiddleLeft,
            Padding = new Padding(8, 0, 8, 0),
            Text = "当前目录（未进入子文件夹）"
        };

        Controls.Add(_list);
        Controls.Add(header);
        Controls.Add(_filters);
        Controls.Add(paths);
        Controls.Add(toolbar);
        Controls.Add(_status);

        _leftBar.LocationCommitted += loc => Commit(loc, _right);
        _rightBar.LocationCommitted += loc => Commit(_left, loc);
        _leftBar.NewConnectionRequested += () => NewConnectionRequested?.Invoke();
        _rightBar.NewConnectionRequested += () => NewConnectionRequested?.Invoke();
        _leftBar.ConnectionsChanged += () => ConnectionsChanged?.Invoke();
        _rightBar.ConnectionsChanged += () => ConnectionsChanged?.Invoke();
        _leftBar.Toast += (m, err) => Toast?.Invoke(m, err);
        _rightBar.Toast += (m, err) => Toast?.Invoke(m, err);

        HighlightFilters();
        UpdateButtons();
        RefreshBars();
    }

    public void SetLocations(LocationRef left, LocationRef right)
    {
        _left = left.Clone();
        _right = right.Clone();
        RefreshBars();
        _ = ListCurrentAsync();
    }

    public void ReloadConnections() => RefreshBars();

    void RefreshBars()
    {
        var cons = _app.Store.ListConnections();
        _leftBar.Bind(_left, cons);
        _rightBar.Bind(_right, cons);
    }

    void Commit(LocationRef left, LocationRef right)
    {
        _left = left.Clone();
        _right = right.Clone();
        RefreshBars();
        var title = LocationPaths.ShortTitle(_left, _right);
        LocationsChanged?.Invoke(_left, _right, title);
        _ = ListCurrentAsync();
    }

    void HighlightFilters()
    {
        foreach (Control c in _filters.Controls)
        {
            if (c is Button b && b.Tag is ViewFilter f)
                b.BackColor = f == _filter ? Theme.Active : Theme.Raise;
        }
    }

    void UpdateButtons()
    {
        var sel = _list.SelectedNodes.ToList();
        _copyRight.Enabled = sel.Any(n => n.Left is not null) && !_busy;
        _copyLeft.Enabled = sel.Any(n => n.Right is not null) && !_busy;
        _syncRight.Enabled = _result is not null && !_busy;
        _syncLeft.Enabled = _result is not null && !_busy;
        _listBtn.Enabled = !_busy;
        _cmpBtn.Enabled = !_busy && LocationPaths.LocReady(Ready(_left)) && LocationPaths.LocReady(Ready(_right));
    }

    void RebuildRows(bool keepSelection = false)
    {
        var ready = LocationPaths.LocReady(_left) || LocationPaths.LocReady(_right);
        if (_result is null)
        {
            _list.EmptyText = ready ? "无法读取目录。检查路径后点「列出」或回车重试。" : "在路径栏输入目录后回车，或点「列出」。";
            _list.SetRows([], keepSelection: keepSelection);
            _list.ShowParentRow = LocationPaths.CanGoUp(_left) || LocationPaths.CanGoUp(_right);
            return;
        }
        var exp = new HashSet<string>();
        CollectCurrentExpanded(_result.Tree, exp);
        var rows = Flatten(_result.Tree, exp, 0);
        _list.ShowParentRow = LocationPaths.CanGoUp(_left) || LocationPaths.CanGoUp(_right);
        _list.EmptyText = rows.Count == 0
            ? "这一视图没有项目。点「全部」，或检查路径后重新列出。"
            : null;
        _list.SetRows(rows, exp, keepSelection);
        UpdateFilterLabels();
        UpdateButtons();
    }

    void CollectCurrentExpanded(List<CompareNode> nodes, HashSet<string> exp)
    {
        foreach (var n in nodes)
        {
            if (n.Type == EntryType.Dir && _list.IsExpanded(n.RelPath))
            {
                exp.Add(n.RelPath);
                if (n.Children is not null) CollectCurrentExpanded(n.Children, exp);
            }
        }
    }

    List<CompareListControl.FlatRow> Flatten(List<CompareNode> nodes, HashSet<string> expanded, int depth)
    {
        var rows = new List<CompareListControl.FlatRow>();
        var q = _query.ToLowerInvariant();
        foreach (var node in nodes)
        {
            if (!MatchesFilter(node, _filter)) continue;
            if (q.Length > 0 && !node.Name.Contains(q, StringComparison.OrdinalIgnoreCase) && !HasQuery(node, q)) continue;
            rows.Add(new CompareListControl.FlatRow { Node = node, Depth = depth });
            if (node.Type == EntryType.Dir && node.Children is not null && expanded.Contains(node.RelPath))
                rows.AddRange(Flatten(node.Children, expanded, depth + 1));
        }
        return rows;
    }

    static bool HasQuery(CompareNode node, string q)
        => node.Name.Contains(q, StringComparison.OrdinalIgnoreCase) || (node.Children?.Any(c => HasQuery(c, q)) ?? false);

    static bool MatchesFilter(CompareNode node, ViewFilter filter) => filter switch
    {
        ViewFilter.All => true,
        ViewFilter.Diff => node.Status != ItemStatus.Same,
        ViewFilter.Same => node.Status == ItemStatus.Same,
        ViewFilter.Left => node.Status == ItemStatus.LeftOnly || HasStatus(node, ItemStatus.LeftOnly),
        ViewFilter.Right => node.Status == ItemStatus.RightOnly || HasStatus(node, ItemStatus.RightOnly),
        _ => true
    };

    static bool HasStatus(CompareNode node, ItemStatus status)
        => node.Status == status || (node.Children?.Any(c => HasStatus(c, status)) ?? false);

    static HashSet<string> CollectExpanded(List<CompareNode> nodes)
    {
        var set = new HashSet<string>();
        void Walk(List<CompareNode> list)
        {
            foreach (var n in list)
            {
                if (n.Type == EntryType.Dir && n.Status != ItemStatus.Same)
                {
                    set.Add(n.RelPath);
                    if (n.Children is not null) Walk(n.Children);
                }
            }
        }
        Walk(nodes);
        return set;
    }

    static void AllDirs(List<CompareNode> nodes, HashSet<string> outSet)
    {
        foreach (var n in nodes)
        {
            if (n.Type == EntryType.Dir)
            {
                outSet.Add(n.RelPath);
                if (n.Children is not null) AllDirs(n.Children, outSet);
            }
        }
    }

    void UpdateFilterLabels()
    {
        var s = _result?.Stats ?? new CompareStats();
        foreach (Control c in _filters.Controls)
        {
            if (c is not Button b || b.Tag is not ViewFilter f) continue;
            b.Text = f switch
            {
                ViewFilter.All => "全部",
                ViewFilter.Diff => $"差异 {s.Different + s.LeftOnly + s.RightOnly}",
                ViewFilter.Left => $"仅左 {s.LeftOnly}",
                ViewFilter.Right => $"仅右 {s.RightOnly}",
                ViewFilter.Same => $"相同 {s.Same}",
                _ => b.Text
            };
        }
        _status.Text = _deep
            ? (_cmpMode == CompareMode.Quick ? "已比较整棵树 · 大小与时间" : "已比较整棵树 · 文件内容 MD5")
            : "当前目录（未进入子文件夹）";
        var shown = _list.Rows.Count;
        var total = (_result?.Stats.Files ?? 0) + (_result?.Stats.Dirs ?? 0);
        _status.Text += $"  ·  {shown}/{total} 项";
        if (_list.SelectedPaths.Count > 0) _status.Text += $"  ·  已选 {_list.SelectedPaths.Count} 项";
    }

    static LocationRef Ready(LocationRef loc)
    {
        if (loc.Kind == LocationKind.Local && string.IsNullOrEmpty(loc.Path))
            return new LocationRef { Kind = LocationKind.Local, Path = "\\" };
        return loc;
    }

    async Task ListCurrentAsync()
    {
        var bothEmpty = string.IsNullOrEmpty(_left.Path) && string.IsNullOrEmpty(_right.Path)
                        && _left.Kind == LocationKind.Local && _right.Kind == LocationKind.Local;
        if (bothEmpty)
        {
            _left.Path = "\\";
            _right.Path = "\\";
        }
        RefreshBars();
        var leftOk = LocationPaths.LocReady(_left);
        var rightOk = LocationPaths.LocReady(_right);
        if (!leftOk && !rightOk)
        {
            _result = null;
            _deep = false;
            RebuildRows();
            _status.Text = "点「列出」查看磁盘，或粘贴路径回车";
            return;
        }
        _busy = true;
        UpdateButtons();
        _status.Text = "正在读取当前目录…";
        try
        {
            if (leftOk) await _app.EnsureConnectedAsync(_left);
            if (rightOk) await _app.EnsureConnectedAsync(_right);
            if (!Live) return;
            var leftTask = leftOk ? TryListAsync(_left) : Task.FromResult(ListAttempt.Empty);
            var rightTask = rightOk ? TryListAsync(_right) : Task.FromResult(ListAttempt.Empty);
            var leftRes = await leftTask;
            var rightRes = await rightTask;
            if (!Live) return;
            if (leftRes.Items.Count == 0 && rightRes.Items.Count == 0 && (leftRes.Error is not null || rightRes.Error is not null))
            {
                _result = null;
                RebuildRows();
                var err = leftRes.Error ?? rightRes.Error ?? "无法读取目录";
                _status.Text = err;
                Toast?.Invoke(err, true);
                return;
            }
            _result = Shallow(leftRes.Items, rightRes.Items);
            _deep = false;
            _list.ClearExpanded();
            RebuildRows();
            if (leftRes.Error is not null || rightRes.Error is not null)
                _status.Text += "  ·  " + (leftRes.Error ?? rightRes.Error);
        }
        catch (ObjectDisposedException) { }
        catch (Exception ex)
        {
            if (!Live) return;
            _result = null;
            RebuildRows();
            _status.Text = ex.Message;
            Toast?.Invoke(ex.Message, true);
        }
        finally
        {
            if (Live)
            {
                _busy = false;
                UpdateButtons();
            }
        }
    }

    async Task RunCompareAsync()
    {
        if (!LocationPaths.LocReady(_left) || !LocationPaths.LocReady(_right))
        {
            Toast?.Invoke("两侧都填好路径后才能比较", true);
            await ListCurrentAsync();
            return;
        }
        _cts?.Cancel();
        _cts = new CancellationTokenSource();
        var ct = _cts.Token;
        _busy = true;
        _deep = true;
        UpdateButtons();
        _status.Text = "正在比较…";
        try
        {
            await _app.EnsureConnectedAsync(_left, ct);
            await _app.EnsureConnectedAsync(_right, ct);
            if (!Live) return;
            var progress = new Progress<(int Scanned, string Path)>(p =>
            {
                Ui(() => _status.Text = $"扫描子目录 {p.Scanned} · {p.Path}");
            });
            _result = await _app.Compare.CompareFoldersAsync(_left, _right, _cmpMode, AppLimits.DefaultIgnore, progress, ct);
            if (!Live) return;
            _filter = ViewFilter.Diff;
            HighlightFilters();
            var exp = CollectExpanded(_result.Tree);
            _list.SetExpanded(exp);
            RebuildRows();
            UpdateFilterLabels();
        }
        catch (OperationCanceledException) { }
        catch (ObjectDisposedException) { }
        catch (Exception ex)
        {
            if (!Live) return;
            _result = null;
            RebuildRows();
            _status.Text = ex.Message;
            Toast?.Invoke(ex.Message, true);
        }
        finally
        {
            if (Live)
            {
                _busy = false;
                UpdateButtons();
            }
        }
    }

    async Task<List<FileMeta>> ListSideAsync(LocationRef loc)
    {
        if (loc.Kind == LocationKind.Local)
            return await Task.Run(() => LocalFs.List(loc.Path));
        return await _app.Sftp.ListAsync(loc.ConnectionId!, string.IsNullOrEmpty(loc.Path) ? "/" : loc.Path);
    }

    async Task<ListAttempt> TryListAsync(LocationRef loc)
    {
        try { return new ListAttempt(await ListSideAsync(loc), null); }
        catch (Exception ex) { return new ListAttempt([], ex.Message); }
    }

    readonly record struct ListAttempt(List<FileMeta> Items, string? Error)
    {
        public static ListAttempt Empty => new([], null);
    }

    static Dictionary<string, FileMeta> MapByName(List<FileMeta> list)
    {
        var map = new Dictionary<string, FileMeta>(StringComparer.OrdinalIgnoreCase);
        foreach (var e in list) map.TryAdd(e.Name, e);
        return map;
    }

    static FolderCompareResult Shallow(List<FileMeta> leftList, List<FileMeta> rightList, string relPrefix = "")
    {
        var leftMap = MapByName(leftList);
        var rightMap = MapByName(rightList);
        var names = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        foreach (var e in leftList) names.Add(e.Name);
        foreach (var e in rightList) names.Add(e.Name);
        var stats = new CompareStats();
        var tree = names.Select(name =>
        {
            leftMap.TryGetValue(name, out var l);
            rightMap.TryGetValue(name, out var r);
            var type = (l ?? r)!.Type;
            var status = ShallowStatus(l, r);
            if (type == EntryType.Dir) stats.Dirs++;
            else
            {
                stats.Files++;
                if (status == ItemStatus.Same) stats.Same++;
                else if (status == ItemStatus.Different) stats.Different++;
                else if (status == ItemStatus.LeftOnly) stats.LeftOnly++;
                else if (status == ItemStatus.RightOnly) stats.RightOnly++;
            }
            var rel = string.IsNullOrEmpty(relPrefix) ? name : relPrefix + "/" + name;
            return new CompareNode { Name = name, RelPath = rel, Type = type, Status = status, Left = l, Right = r };
        }).OrderBy(n => n.Type == EntryType.Dir ? 0 : 1)
          .ThenBy(n => n.Name, StringComparer.Create(TextUtil.Zh, true))
          .ToList();
        return new FolderCompareResult { Tree = tree, Stats = stats };
    }

    static ItemStatus ShallowStatus(FileMeta? left, FileMeta? right)
    {
        if (left is not null && right is null) return ItemStatus.LeftOnly;
        if (left is null && right is not null) return ItemStatus.RightOnly;
        if (left is null || right is null) return ItemStatus.Unknown;
        if (left.Type != right.Type) return ItemStatus.Different;
        if (left.Type == EntryType.Dir) return ItemStatus.Unknown;
        if (left.Size != right.Size) return ItemStatus.Different;
        if (left.Mtime / 1000 != right.Mtime / 1000) return ItemStatus.Different;
        return ItemStatus.Same;
    }

    void OpenNode(CompareNode node)
    {
        if (node.Type == EntryType.Dir)
        {
            _list.ToggleExpand(node);
            return;
        }
        var l = node.Left is not null ? LocationPaths.ChildLoc(_left, node.Name, node.Left.Path) : new LocationRef { Kind = _left.Kind, ConnectionId = _left.ConnectionId };
        var r = node.Right is not null ? LocationPaths.ChildLoc(_right, node.Name, node.Right.Path) : new LocationRef { Kind = _right.Kind, ConnectionId = _right.ConnectionId };
        OpenFile?.Invoke(l, r, node.Name);
    }

    void EnterDir(CompareNode node)
    {
        if (node.Type != EntryType.Dir) return;
        var nextLeft = node.Left is not null
            ? LocationPaths.WithPath(_left, node.Left.Path)
            : _left;
        var nextRight = node.Right is not null
            ? LocationPaths.WithPath(_right, node.Right.Path)
            : _right;
        nextLeft.DisplayUrl = null;
        nextRight.DisplayUrl = null;
        Commit(nextLeft, nextRight);
    }

    async Task ExpandAllAsync()
    {
        if (_result is null || !Live) return;
        foreach (var dir in TopDirs(_result.Tree))
        {
            if (!Live) return;
            if (dir.Children is not null) continue;
            try { await LoadChildrenAsync(dir); }
            catch { dir.Children = []; }
        }
        if (!Live) return;
        var s = new HashSet<string>();
        AllDirs(_result.Tree, s);
        _list.ExpandAll(s);
        RebuildRows(true);
    }

    static IEnumerable<CompareNode> TopDirs(List<CompareNode> nodes)
    {
        foreach (var n in nodes)
        {
            if (n.Type == EntryType.Dir) yield return n;
        }
    }

    async Task OnExpandToggledAsync(CompareNode node)
    {
        if (!Live) return;
        if (node.Type != EntryType.Dir)
        {
            RebuildRows(true);
            return;
        }
        if (_list.IsExpanded(node.RelPath) && node.Children is null)
        {
            try
            {
                await LoadChildrenAsync(node);
            }
            catch (ObjectDisposedException) { return; }
            catch (Exception ex)
            {
                if (!Live) return;
                Toast?.Invoke(ex.Message, true);
                node.Children = [];
            }
        }
        if (!Live) return;
        RebuildRows(true);
    }

    async Task LoadChildrenAsync(CompareNode node)
    {
        LocationRef? leftLoc = node.Left is null ? null : new LocationRef
        {
            Kind = _left.Kind,
            Path = node.Left.Path,
            ConnectionId = _left.ConnectionId
        };
        LocationRef? rightLoc = node.Right is null ? null : new LocationRef
        {
            Kind = _right.Kind,
            Path = node.Right.Path,
            ConnectionId = _right.ConnectionId
        };
        if (leftLoc is not null) await _app.EnsureConnectedAsync(leftLoc);
        if (rightLoc is not null) await _app.EnsureConnectedAsync(rightLoc);
        if (!Live) return;
        var leftList = leftLoc is null ? [] : (await TryListAsync(leftLoc)).Items;
        var rightList = rightLoc is null ? [] : (await TryListAsync(rightLoc)).Items;
        if (!Live) return;
        node.Children = Shallow(leftList, rightList, node.RelPath).Tree;
    }

    void GoUp()
    {
        Commit(
            LocationPaths.CanGoUp(_left) ? LocationPaths.ParentPath(_left) : _left,
            LocationPaths.CanGoUp(_right) ? LocationPaths.ParentPath(_right) : _right);
    }

    async Task CopySelectedAsync(bool toRight)
        => await CopyNodesAsync(_list.SelectedNodes.ToList(), toRight);

    async Task CopyNodesAsync(List<CompareNode> nodes, bool toRight)
    {
        var list = nodes.Where(n => toRight ? n.Left is not null : n.Right is not null).ToList();
        if (list.Count == 0)
        {
            Toast?.Invoke("没有可复制的项目", true);
            return;
        }
        _busy = true;
        UpdateButtons();
        try
        {
            foreach (var node in list)
            {
                if (!Live) return;
                var srcMeta = toRight ? node.Left : node.Right;
                if (srcMeta is null) continue;
                var source = toRight
                    ? new LocationRef { Kind = _left.Kind, Path = srcMeta.Path, ConnectionId = _left.ConnectionId }
                    : new LocationRef { Kind = _right.Kind, Path = srcMeta.Path, ConnectionId = _right.ConnectionId };
                var parentRel = string.Join('/', node.RelPath.Split('/').SkipLast(1));
                var targetDir = LocationPaths.JoinRel(toRight ? _right : _left, parentRel);
                await _app.EnsureConnectedAsync(source);
                await _app.EnsureConnectedAsync(targetDir);
                await _app.Transfer.CopyAsync(source, targetDir, node.Type == EntryType.Dir);
            }
            if (!Live) return;
            Toast?.Invoke(toRight ? $"已复制 {list.Count} 项到右侧" : $"已复制 {list.Count} 项到左侧", false);
            if (_deep) await RunCompareAsync();
            else await ListCurrentAsync();
        }
        catch (ObjectDisposedException) { }
        catch (Exception ex) { if (Live) Toast?.Invoke(ex.Message, true); }
        finally
        {
            if (Live)
            {
                _busy = false;
                UpdateButtons();
            }
        }
    }

    async Task SyncAsync(bool toRight)
    {
        if (_result is null) return;
        var items = new List<CompareNode>();
        CollectSync(_result.Tree, toRight, items);
        if (items.Count == 0)
        {
            Toast?.Invoke("没有需要同步的差异", false);
            return;
        }
        var label = toRight ? "右侧" : "左侧";
        if (MessageBox.Show(this, $"把 {items.Count} 个差异项复制到{label}？", "同步", MessageBoxButtons.OKCancel, MessageBoxIcon.Question) != DialogResult.OK)
            return;
        await CopyNodesAsync(items, toRight);
    }

    static void CollectSync(List<CompareNode> nodes, bool toRight, List<CompareNode> outList)
    {
        foreach (var n in nodes)
        {
            var take = toRight
                ? (n.Status is ItemStatus.LeftOnly or ItemStatus.Different) && n.Left is not null
                : (n.Status is ItemStatus.RightOnly or ItemStatus.Different) && n.Right is not null;
            if (take && (n.Type == EntryType.File || n.Status is ItemStatus.LeftOnly or ItemStatus.RightOnly))
            {
                outList.Add(n);
                continue;
            }
            if (n.Children is not null) CollectSync(n.Children, toRight, outList);
        }
    }

    public async Task RefreshAsync()
    {
        if (_deep) await RunCompareAsync();
        else await ListCurrentAsync();
    }

    protected override bool ProcessCmdKey(ref Message msg, Keys keyData)
    {
        if (keyData == Keys.F5)
        {
            _ = RefreshAsync();
            return true;
        }
        return base.ProcessCmdKey(ref msg, keyData);
    }

    bool Live => !IsDisposed && IsHandleCreated;

    void Ui(Action action)
    {
        if (!Live) return;
        try
        {
            if (InvokeRequired) BeginInvoke(() => { if (Live) action(); });
            else action();
        }
        catch (ObjectDisposedException) { }
        catch (InvalidOperationException) { }
    }

    protected override void Dispose(bool disposing)
    {
        if (disposing)
        {
            try { _cts?.Cancel(); } catch { /* ignore */ }
            _cts?.Dispose();
            _cts = null;
        }
        base.Dispose(disposing);
    }
}
