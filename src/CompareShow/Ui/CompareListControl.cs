using CompareShow.Core;

namespace CompareShow.Ui;

public sealed class CompareListControl : Control
{
    public sealed class FlatRow
    {
        public required CompareNode Node { get; init; }
        public int Depth { get; init; }
    }

    readonly VScrollBar _scroll = new() { Dock = DockStyle.Right };
    readonly List<FlatRow> _rows = [];
    readonly HashSet<string> _expanded = [];
    readonly HashSet<string> _selected = [];
    string? _anchor;
    int _hover = -1;

    public IReadOnlyList<FlatRow> Rows => _rows;
    public IReadOnlyCollection<string> SelectedPaths => _selected;
    public CompareNode? AnchorNode => _rows.FirstOrDefault(r => r.Node.RelPath == _anchor)?.Node;
    public IEnumerable<CompareNode> SelectedNodes => _rows.Where(r => _selected.Contains(r.Node.RelPath)).Select(r => r.Node);

    public event Action<CompareNode>? NodeActivated;
    public event Action<CompareNode>? DirectoryEntered;
    public event Action<CompareNode, bool>? CopyRequested;
    public event Action? GoUpRequested;
    public event Action? SelectionChanged;
    public event Action<CompareNode>? ExpandToggled;

    public CompareListControl()
    {
        SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.OptimizedDoubleBuffer | ControlStyles.ResizeRedraw | ControlStyles.UserPaint, true);
        TabStop = true;
        BackColor = Theme.Bg;
        Controls.Add(_scroll);
        _scroll.Scroll += (_, _) => Invalidate();
        MouseWheel += (_, e) =>
        {
            SetScrollValue(_scroll.Value - Math.Sign(e.Delta) * RowHeight * 3);
            Invalidate();
        };
        ContextMenuStrip = BuildMenu();
    }

    public int RowHeight => 26;
    public bool ShowParentRow { get; set; }
    public string? EmptyText { get; set; }

    public void SetRows(IEnumerable<FlatRow> rows, IEnumerable<string>? expanded = null, bool keepSelection = false)
    {
        _rows.Clear();
        _rows.AddRange(rows);
        if (expanded is not null)
        {
            _expanded.Clear();
            foreach (var p in expanded) _expanded.Add(p);
        }
        if (!keepSelection)
        {
            _selected.Clear();
            _anchor = null;
        }
        else
        {
            _selected.RemoveWhere(p => _rows.All(r => r.Node.RelPath != p));
            if (_anchor is not null && _rows.All(r => r.Node.RelPath != _anchor)) _anchor = null;
        }
        UpdateScroll();
        Invalidate();
        SelectionChanged?.Invoke();
    }

    public void SetExpanded(IEnumerable<string> paths)
    {
        _expanded.Clear();
        foreach (var p in paths) _expanded.Add(p);
        Invalidate();
    }

    public void ClearExpanded()
    {
        _expanded.Clear();
        Invalidate();
    }

    public bool IsExpanded(string path) => _expanded.Contains(path);

    public void ToggleExpand(CompareNode node)
    {
        if (node.Type != EntryType.Dir) return;
        if (!_expanded.Add(node.RelPath)) _expanded.Remove(node.RelPath);
        ExpandToggled?.Invoke(node);
    }

    public void ExpandAll(IEnumerable<string> paths)
    {
        foreach (var p in paths) _expanded.Add(p);
        Invalidate();
    }

    void UpdateScroll()
    {
        var view = Math.Max(1, Height);
        var content = Math.Max(view, VisibleCount() * Math.Max(1, RowHeight));
        _scroll.Minimum = 0;
        _scroll.SmallChange = Math.Max(1, RowHeight);
        _scroll.LargeChange = view;
        _scroll.Maximum = Math.Max(0, content - 1);
        SetScrollValue(_scroll.Value);
    }

    void SetScrollValue(int value)
    {
        try
        {
            if (!IsHandleCreated || _scroll.IsDisposed) return;
            var usable = Math.Max(_scroll.Minimum, _scroll.Maximum - Math.Max(1, _scroll.LargeChange) + 1);
            var v = Math.Clamp(value, _scroll.Minimum, usable);
            if (_scroll.Value != v) _scroll.Value = v;
        }
        catch
        {
            /* VScrollBar 在句柄销毁或 LargeChange 变化时会抛 */
        }
    }

    int VisibleCount() => _rows.Count + (ShowParentRow ? 1 : 0);

    int FirstVisible() => Math.Max(0, _scroll.Value / RowHeight);

    protected override void OnResize(EventArgs e)
    {
        base.OnResize(e);
        UpdateScroll();
    }

    protected override void OnPaint(PaintEventArgs e)
    {
        try
        {
            PaintRows(e.Graphics);
        }
        catch (Exception ex)
        {
            try
            {
                e.Graphics.Clear(Theme.Bg);
                TextRenderer.DrawText(e.Graphics, "列表绘制失败：" + ex.Message, Theme.Ui, SafeRect(12, 12, Math.Max(1, Width - 24), 80), Theme.Danger, TextFormatFlags.WordBreak);
            }
            catch { /* ignore */ }
        }
    }

    void PaintRows(Graphics g)
    {
        g.Clear(Theme.Bg);
        var w = Math.Max(1, Width - _scroll.Width);
        var mid = Math.Max(40, w / 2);
        var rowH = Math.Max(1, RowHeight);
        using var line = new Pen(Theme.Line);

        var y = 0;
        var start = FirstVisible();
        var count = VisibleCount();
        for (var i = start; i < count && y < Height; i++)
        {
            var rect = SafeRect(0, y, w, rowH);
            if (ShowParentRow && i == 0)
            {
                using var b = new SolidBrush(Theme.Raise);
                g.FillRectangle(b, rect);
                DrawName(g, 8, y, "..", true, 0, Theme.Muted);
                DrawName(g, mid + 8, y, "..", true, 0, Theme.Muted);
                if (mid > 0 && mid < w) g.DrawLine(line, mid, y, mid, y + rowH);
                y += rowH;
                continue;
            }
            var rowIndex = ShowParentRow ? i - 1 : i;
            if (rowIndex < 0 || rowIndex >= _rows.Count) break;
            var row = _rows[rowIndex];
            var selected = _selected.Contains(row.Node.RelPath);
            var bg = selected ? Theme.Selected : Theme.StatusBack(row.Node.Status);
            if (!selected && i == _hover) bg = Theme.Hover;
            using (var b = new SolidBrush(bg)) g.FillRectangle(b, rect);
            var paneW = Math.Max(1, mid - 28);
            DrawPane(g, row, true, 0, y, paneW, rowH);
            DrawCenter(g, row, mid - 28, y, 56, rowH);
            DrawPane(g, row, false, mid + 28, y, Math.Max(1, w - mid - 28), rowH);
            if (mid > 0 && mid < w) g.DrawLine(line, mid, y, mid, y + rowH);
            y += rowH;
        }
        if (w > 1) g.DrawLine(line, w - 1, 0, w - 1, Height);
        if (_rows.Count == 0)
        {
            var msg = string.IsNullOrEmpty(EmptyText) ? "没有可显示的项目" : EmptyText;
            TextRenderer.DrawText(g, msg, Theme.Ui, SafeRect(24, 36, w - 48, 80), Theme.Muted, TextFormatFlags.WordBreak);
        }
    }

    void DrawCenter(Graphics g, FlatRow row, int x, int y, int w, int h)
    {
        var canRight = row.Node.Left is not null && row.Node.Status != ItemStatus.Same;
        var canLeft = row.Node.Right is not null && row.Node.Status != ItemStatus.Same;
        TextRenderer.DrawText(g, "→", Theme.Ui, SafeRect(x, y, w / 2, h), canRight ? Theme.Accent : Theme.Faint, TextFormatFlags.HorizontalCenter | TextFormatFlags.VerticalCenter);
        TextRenderer.DrawText(g, "←", Theme.Ui, SafeRect(x + w / 2, y, w / 2, h), canLeft ? Theme.Accent : Theme.Faint, TextFormatFlags.HorizontalCenter | TextFormatFlags.VerticalCenter);
    }

    void DrawPane(Graphics g, FlatRow row, bool left, int x, int y, int w, int h)
    {
        if (w <= 0 || h <= 0) return;
        var meta = left ? row.Node.Left : row.Node.Right;
        var missing = meta is null;
        var color = Theme.StatusFore(row.Node.Status, missing);
        var name = meta?.Name ?? "";
        DrawName(g, x + 6, y, name, row.Node.Type == EntryType.Dir, row.Depth, color, _expanded.Contains(row.Node.RelPath));
        var size = meta is not null && row.Node.Type == EntryType.File ? TextUtil.FormatSize(meta.Size) : "";
        var time = meta is not null && row.Node.Type == EntryType.File ? TextUtil.FormatTime(meta.Mtime) : "";
        var timeW = Math.Min(108, Math.Max(0, w / 3));
        var sizeW = Math.Min(72, Math.Max(0, w / 4));
        TextRenderer.DrawText(g, time, Theme.Small, SafeRect(x + w - timeW, y, timeW, h), Theme.Faint, TextFormatFlags.Right | TextFormatFlags.VerticalCenter | TextFormatFlags.EndEllipsis);
        TextRenderer.DrawText(g, size, Theme.Small, SafeRect(x + w - timeW - sizeW, y, sizeW, h), Theme.Faint, TextFormatFlags.Right | TextFormatFlags.VerticalCenter | TextFormatFlags.EndEllipsis);
    }

    void DrawName(Graphics g, int x, int y, string name, bool isDir, int depth, Color color, bool expanded = false)
    {
        var indent = depth * 14;
        var prefix = isDir ? (expanded ? "▾ " : "▸ ") : "  ";
        TextRenderer.DrawText(g, prefix + name, Theme.Ui, SafeRect(x + indent, y, 2000, RowHeight), color, TextFormatFlags.VerticalCenter | TextFormatFlags.EndEllipsis);
    }

    static Rectangle SafeRect(int x, int y, int w, int h)
        => new(x, y, Math.Max(1, w), Math.Max(1, h));

    protected override void OnMouseMove(MouseEventArgs e)
    {
        var i = HitIndex(e.Y);
        if (i != _hover) { _hover = i; Invalidate(); }
        var w = Width - _scroll.Width;
        var mid = w / 2;
        Cursor = Math.Abs(e.X - mid) < 28 ? Cursors.Hand : Cursors.Default;
    }

    protected override void OnMouseLeave(EventArgs e)
    {
        _hover = -1;
        Invalidate();
    }

    protected override void OnMouseDown(MouseEventArgs e)
    {
        Focus();
        var idx = HitIndex(e.Y);
        if (idx < 0) return;
        if (ShowParentRow && idx == 0)
        {
            if (e.Clicks >= 2) GoUpRequested?.Invoke();
            return;
        }
        var rowIndex = ShowParentRow ? idx - 1 : idx;
        if (rowIndex < 0 || rowIndex >= _rows.Count) return;
        var row = _rows[rowIndex];
        var w = Width - _scroll.Width;
        var mid = w / 2;

        if (e.Button == MouseButtons.Left && e.X >= mid - 28 && e.X < mid)
        {
            if (row.Node.Left is not null && row.Node.Status != ItemStatus.Same)
                CopyRequested?.Invoke(row.Node, true);
            return;
        }
        if (e.Button == MouseButtons.Left && e.X >= mid && e.X < mid + 28)
        {
            if (row.Node.Right is not null && row.Node.Status != ItemStatus.Same)
                CopyRequested?.Invoke(row.Node, false);
            return;
        }

        if (e.Clicks >= 2 && e.Button == MouseButtons.Left)
        {
            SelectOne(row.Node.RelPath);
            if (row.Node.Type == EntryType.Dir)
            {
                if (!HitTwist(row.Depth, e.X, mid)) ToggleExpand(row.Node);
            }
            else NodeActivated?.Invoke(row.Node);
            return;
        }

        if (e.Button == MouseButtons.Left)
        {
            if (row.Node.Type == EntryType.Dir && HitTwist(row.Depth, e.X, mid))
            {
                SelectOne(row.Node.RelPath);
                ToggleExpand(row.Node);
                return;
            }
            SelectRow(row.Node.RelPath, ModifierKeys);
        }
        else if (e.Button == MouseButtons.Right)
        {
            if (!_selected.Contains(row.Node.RelPath)) SelectOne(row.Node.RelPath);
        }
    }

    void SelectOne(string path)
    {
        _selected.Clear();
        _selected.Add(path);
        _anchor = path;
        Invalidate();
        SelectionChanged?.Invoke();
    }

    void SelectRow(string path, Keys mods)
    {
        if ((mods & Keys.Shift) == Keys.Shift && _anchor is not null)
        {
            var i1 = _rows.FindIndex(r => r.Node.RelPath == _anchor);
            var i2 = _rows.FindIndex(r => r.Node.RelPath == path);
            if (i1 >= 0 && i2 >= 0)
            {
                var a = Math.Min(i1, i2);
                var b = Math.Max(i1, i2);
                _selected.Clear();
                for (var i = a; i <= b; i++) _selected.Add(_rows[i].Node.RelPath);
                Invalidate();
                SelectionChanged?.Invoke();
                return;
            }
        }
        if ((mods & Keys.Control) == Keys.Control)
        {
            if (!_selected.Add(path)) _selected.Remove(path);
            _anchor = path;
        }
        else SelectOne(path);
        Invalidate();
        SelectionChanged?.Invoke();
    }

    public void SelectAll()
    {
        _selected.Clear();
        foreach (var r in _rows) _selected.Add(r.Node.RelPath);
        Invalidate();
        SelectionChanged?.Invoke();
    }

    int HitIndex(int y) => y < 0 ? -1 : FirstVisible() + y / RowHeight;

    static bool HitTwist(int depth, int x, int mid)
    {
        var indent = depth * 14;
        var left = 4 + indent;
        var right = mid + 32 + indent;
        return (x >= left && x <= left + 28) || (x >= right && x <= right + 28);
    }

    protected override bool IsInputKey(Keys keyData)
    {
        return keyData is Keys.Up or Keys.Down or Keys.Left or Keys.Right or Keys.Enter or Keys.Back
            || base.IsInputKey(keyData);
    }

    protected override void OnKeyDown(KeyEventArgs e)
    {
        if (e.KeyCode == Keys.A && e.Control) { SelectAll(); e.Handled = true; return; }
        if (e.KeyCode == Keys.Back) { GoUpRequested?.Invoke(); e.Handled = true; return; }
        if (e.KeyCode is Keys.Up or Keys.Down)
        {
            if (_rows.Count == 0) return;
            var cur = _anchor is null ? -1 : _rows.FindIndex(r => r.Node.RelPath == _anchor);
            var next = e.KeyCode == Keys.Down ? Math.Min(_rows.Count - 1, cur + 1) : Math.Max(0, cur < 0 ? 0 : cur - 1);
            SelectRow(_rows[next].Node.RelPath, e.Shift ? Keys.Shift : Keys.None);
            EnsureVisible(next);
            e.Handled = true;
            return;
        }
        if (e.KeyCode == Keys.Enter && AnchorNode is { } n)
        {
            if (e.Control && n.Type == EntryType.Dir) DirectoryEntered?.Invoke(n);
            else if (n.Type == EntryType.Dir) ToggleExpand(n);
            else NodeActivated?.Invoke(n);
            e.Handled = true;
            return;
        }
        if (e.KeyCode == Keys.Right && AnchorNode is { } r)
        {
            if (e.Control || r.Type == EntryType.File) CopyRequested?.Invoke(r, true);
            else if (!_expanded.Contains(r.RelPath)) ToggleExpand(r);
            e.Handled = true;
            return;
        }
        if (e.KeyCode == Keys.Left && AnchorNode is { } l)
        {
            if (e.Control || l.Type == EntryType.File) CopyRequested?.Invoke(l, false);
            else if (_expanded.Contains(l.RelPath)) ToggleExpand(l);
            e.Handled = true;
            return;
        }
    }

    void EnsureVisible(int rowIndex)
    {
        var visual = rowIndex + (ShowParentRow ? 1 : 0);
        var top = visual * RowHeight;
        if (top < _scroll.Value) SetScrollValue(top);
        else if (top + RowHeight > _scroll.Value + Height)
            SetScrollValue(top + RowHeight - Height);
        Invalidate();
    }

    ContextMenuStrip BuildMenu()
    {
        var m = new ContextMenuStrip();
        m.Opening += (_, e) =>
        {
            m.Items.Clear();
            var n = AnchorNode;
            if (n is null) { e.Cancel = true; return; }
            if (n.Type == EntryType.File)
                m.Items.Add("比较内容", null, (_, _) => NodeActivated?.Invoke(n));
            if (n.Type == EntryType.Dir)
            {
                m.Items.Add("展开 / 折叠", null, (_, _) => ToggleExpand(n));
                m.Items.Add("进入此文件夹", null, (_, _) => DirectoryEntered?.Invoke(n));
            }
            m.Items.Add("复制到右侧", null, (_, _) => CopyRequested?.Invoke(n, true)).Enabled = n.Left is not null;
            m.Items.Add("复制到左侧", null, (_, _) => CopyRequested?.Invoke(n, false)).Enabled = n.Right is not null;
        };
        return m;
    }
}
