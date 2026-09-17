using DiffPlex;
using DiffPlex.DiffBuilder;
using DiffPlex.DiffBuilder.Model;

namespace CompareShow.Ui;

public sealed class LineDiffControl : Control
{
    public sealed class Row
    {
        public string Left = "";
        public string Right = "";
        public ChangeType LeftKind;
        public ChangeType RightKind;
        public bool IsDiff => LeftKind != ChangeType.Unchanged || RightKind != ChangeType.Unchanged;
    }

    const int Gutter = 8;
    const int LineNoW = 48;
    const int SplitGap = 8;

    readonly VScrollBar _v = new();
    readonly HScrollBar _h = new();
    readonly List<Row> _rows = [];
    int[] _leftNo = [];
    int[] _rightNo = [];
    int _selected;
    int _hover = -1;
    int _maxChars = 40;
    bool _focusLeft = true;
    string _nl = "\n";
    bool _leftEndsNl;
    bool _rightEndsNl;
    TextBox? _edit;
    int _editIndex = -1;
    bool _editLeft;
    bool _suspendLostFocus;

    public event Action? Changed;

    public LineDiffControl()
    {
        SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.OptimizedDoubleBuffer | ControlStyles.ResizeRedraw | ControlStyles.UserPaint, true);
        TabStop = true;
        BackColor = Theme.Bg;
        Controls.Add(_v);
        Controls.Add(_h);
        _v.Scroll += (_, _) => { PlaceEdit(); Invalidate(); };
        _h.Scroll += (_, _) => { PlaceEdit(); Invalidate(); };
        MouseWheel += (_, e) =>
        {
            SetV(_v.Value - Math.Sign(e.Delta) * RowHeight * 3);
            PlaceEdit();
            Invalidate();
        };
        ContextMenuStrip = BuildMenu();
    }

    public int RowHeight => 22;
    public IReadOnlyList<Row> Rows => _rows;
    public int SelectedIndex => _selected;
    public bool DirtyLeft { get; private set; }
    public bool DirtyRight { get; private set; }
    public int DiffCount => _rows.Count(r => r.IsDiff);

    public void ClearDirty(bool left, bool right)
    {
        if (left) DirtyLeft = false;
        if (right) DirtyRight = false;
        Changed?.Invoke();
        Invalidate();
    }

    public void LoadTexts(string left, string right, bool ignoreWs, bool keepDirty = false)
    {
        CommitEdit();
        _nl = left.Contains("\r\n") || right.Contains("\r\n") ? "\r\n" : "\n";
        _leftEndsNl = left.EndsWith('\n');
        _rightEndsNl = right.EndsWith('\n');
        var diff = SideBySideDiffBuilder.Diff(new Differ(), left, right, ignoreWhiteSpace: ignoreWs, ignoreCase: false);
        var n = Math.Max(diff.OldText.Lines.Count, diff.NewText.Lines.Count);
        _rows.Clear();
        _maxChars = 8;
        for (var i = 0; i < n; i++)
        {
            var o = i < diff.OldText.Lines.Count ? diff.OldText.Lines[i] : null;
            var nw = i < diff.NewText.Lines.Count ? diff.NewText.Lines[i] : null;
            var row = new Row
            {
                Left = o?.Text ?? "",
                Right = nw?.Text ?? "",
                LeftKind = o?.Type ?? ChangeType.Imaginary,
                RightKind = nw?.Type ?? ChangeType.Imaginary
            };
            _rows.Add(row);
            _maxChars = Math.Max(_maxChars, Math.Max(row.Left.Length, row.Right.Length));
        }
        if (_rows.Count == 0)
        {
            _rows.Add(new Row { LeftKind = ChangeType.Unchanged, RightKind = ChangeType.Unchanged });
        }
        Reindex();
        if (!keepDirty) DirtyLeft = DirtyRight = false;
        _selected = 0;
        _hover = -1;
        LayoutBars();
        UpdateScroll();
        Invalidate();
        Changed?.Invoke();
    }

    public void Realign(bool ignoreWs)
    {
        var left = BuildLeft();
        var right = BuildRight();
        LoadTexts(left, right, ignoreWs, keepDirty: true);
    }

    public string BuildLeft() { CommitEdit(); return BuildSide(true); }
    public string BuildRight() { CommitEdit(); return BuildSide(false); }

    string BuildSide(bool left)
    {
        var parts = new List<string>(_rows.Count);
        foreach (var r in _rows)
        {
            if (left)
            {
                if (r.LeftKind == ChangeType.Imaginary) continue;
                parts.Add(r.Left);
            }
            else
            {
                if (r.RightKind == ChangeType.Imaginary) continue;
                parts.Add(r.Right);
            }
        }
        var s = string.Join(_nl, parts);
        var ends = left ? _leftEndsNl : _rightEndsNl;
        if (ends && parts.Count > 0 && !s.EndsWith('\n')) s += _nl;
        return s;
    }

    public bool Jump(int dir)
    {
        if (_rows.Count == 0) return false;
        CommitEdit();
        var start = _selected;
        for (var n = 1; n <= _rows.Count; n++)
        {
            var i = (start + dir * n + _rows.Count * 16) % _rows.Count;
            if (_rows[i].IsDiff)
            {
                Select(i);
                BeginEdit(_focusLeft);
                return true;
            }
        }
        return false;
    }

    public void CopyLine(bool toRight)
    {
        if (_selected < 0 || _selected >= _rows.Count) return;
        CommitEdit();
        ApplyCopy(_rows[_selected], toRight);
        AfterEdit(toRight);
    }

    public void CopyHunk(bool toRight)
    {
        if (_selected < 0 || _selected >= _rows.Count) return;
        CommitEdit();
        var (from, to) = HunkRange(_selected);
        for (var i = from; i <= to; i++)
            ApplyCopy(_rows[i], toRight);
        AfterEdit(toRight);
    }

    public void BeginEdit(bool left, int? caret = null, bool selectAll = false)
    {
        if (_rows.Count == 0)
        {
            _rows.Add(new Row { LeftKind = ChangeType.Unchanged, RightKind = ChangeType.Unchanged });
            Reindex();
            _selected = 0;
        }
        if (_selected < 0 || _selected >= _rows.Count) return;
        if (_edit is not null && _editIndex == _selected && _editLeft == left)
        {
            if (caret is int c)
            {
                _edit.SelectionStart = Math.Clamp(c, 0, _edit.TextLength);
                _edit.SelectionLength = 0;
            }
            _edit.Focus();
            return;
        }
        CommitEdit();
        _focusLeft = left;
        _editIndex = _selected;
        _editLeft = left;
        var row = _rows[_selected];
        if (left && row.LeftKind == ChangeType.Imaginary) row.LeftKind = ChangeType.Deleted;
        if (!left && row.RightKind == ChangeType.Imaginary) row.RightKind = ChangeType.Inserted;
        var box = EnsureEdit();
        _suspendLostFocus = true;
        box.Text = left ? row.Left : row.Right;
        PlaceEdit();
        if (selectAll) box.SelectAll();
        else
        {
            var col = caret ?? Math.Min(box.TextLength, box.TextLength);
            box.SelectionStart = Math.Clamp(col, 0, box.TextLength);
            box.SelectionLength = 0;
        }
        box.Focus();
        _suspendLostFocus = false;
        Invalidate();
    }

    TextBox EnsureEdit()
    {
        if (_edit is not null) return _edit;
        var box = new TextBox
        {
            BorderStyle = BorderStyle.None,
            Font = Theme.Mono,
            BackColor = Color.FromArgb(18, 22, 30),
            ForeColor = Theme.Text,
            AcceptsTab = true
        };
        box.KeyDown += EditKeyDown;
        box.LostFocus += (_, _) =>
        {
            if (_suspendLostFocus) return;
            CommitEdit();
        };
        Controls.Add(box);
        _edit = box;
        return box;
    }

    void EditKeyDown(object? sender, KeyEventArgs e)
    {
        if (_edit is null) return;
        if (e.KeyCode == Keys.Escape)
        {
            e.Handled = true;
            e.SuppressKeyPress = true;
            CancelEdit();
            return;
        }
        if (e.KeyCode == Keys.Enter && !e.Control)
        {
            e.Handled = true;
            e.SuppressKeyPress = true;
            SplitLine();
            return;
        }
        if (e.KeyCode == Keys.Up)
        {
            e.Handled = true;
            MoveEdit(-1, _edit.SelectionStart);
            return;
        }
        if (e.KeyCode == Keys.Down)
        {
            e.Handled = true;
            MoveEdit(1, _edit.SelectionStart);
            return;
        }
        if (e.KeyCode == Keys.Back && _edit.SelectionStart == 0 && _edit.SelectionLength == 0)
        {
            e.Handled = true;
            e.SuppressKeyPress = true;
            MergeWithPrevious();
            return;
        }
        if (e.KeyCode == Keys.Delete && _edit.SelectionStart == _edit.TextLength && _edit.SelectionLength == 0)
        {
            e.Handled = true;
            MergeWithNext();
            return;
        }
        if (e.KeyCode == Keys.Left && _edit.SelectionStart == 0 && _edit.SelectionLength == 0)
        {
            e.Handled = true;
            MoveEdit(-1, int.MaxValue);
            return;
        }
        if (e.KeyCode == Keys.Right && _edit.SelectionStart == _edit.TextLength && _edit.SelectionLength == 0)
        {
            e.Handled = true;
            MoveEdit(1, 0);
            return;
        }
    }

    void SplitLine()
    {
        if (_edit is null || _editIndex < 0 || _editIndex >= _rows.Count) return;
        var col = Math.Clamp(_edit.SelectionStart, 0, _edit.TextLength);
        var text = _edit.Text;
        var head = text[..col];
        var tail = text[col..];
        var left = _editLeft;
        var at = _editIndex;
        _edit.Text = head;
        CommitEdit();
        var inserted = new Row();
        if (left)
        {
            inserted.Left = tail;
            inserted.LeftKind = ChangeType.Deleted;
            inserted.RightKind = ChangeType.Imaginary;
            DirtyLeft = true;
        }
        else
        {
            inserted.Right = tail;
            inserted.RightKind = ChangeType.Inserted;
            inserted.LeftKind = ChangeType.Imaginary;
            DirtyRight = true;
        }
        RecalcRow(inserted);
        _rows.Insert(at + 1, inserted);
        Reindex();
        UpdateScroll();
        Select(at + 1);
        BeginEdit(left, 0);
        Changed?.Invoke();
    }

    void MergeWithPrevious()
    {
        if (_edit is null) return;
        var cur = _editIndex;
        var left = _editLeft;
        var rest = _edit.Text;
        var prev = PrevPresent(cur, left);
        if (prev < 0) return;
        var prevText = left ? _rows[prev].Left : _rows[prev].Right;
        var caret = prevText.Length;
        CommitEdit(false);
        ApplyBoxToRow(prev, left, prevText + rest);
        RemoveSide(cur, left);
        Reindex();
        UpdateScroll();
        if (_rows.Count == 0) return;
        Select(Math.Min(prev, _rows.Count - 1));
        BeginEdit(left, caret);
        Changed?.Invoke();
    }

    void MergeWithNext()
    {
        if (_edit is null) return;
        var cur = _editIndex;
        var left = _editLeft;
        var head = _edit.Text;
        var next = NextPresent(cur, left);
        if (next < 0) return;
        var nextText = left ? _rows[next].Left : _rows[next].Right;
        CommitEdit(false);
        ApplyBoxToRow(cur, left, head + nextText);
        RemoveSide(next, left);
        Reindex();
        UpdateScroll();
        Select(Math.Min(cur, Math.Max(0, _rows.Count - 1)));
        BeginEdit(left, head.Length);
        Changed?.Invoke();
    }

    int PrevPresent(int from, bool left)
    {
        for (var i = from - 1; i >= 0; i--)
        {
            if (left ? _rows[i].LeftKind != ChangeType.Imaginary : _rows[i].RightKind != ChangeType.Imaginary)
                return i;
        }
        return -1;
    }

    int NextPresent(int from, bool left)
    {
        for (var i = from + 1; i < _rows.Count; i++)
        {
            if (left ? _rows[i].LeftKind != ChangeType.Imaginary : _rows[i].RightKind != ChangeType.Imaginary)
                return i;
        }
        return -1;
    }

    void RemoveSide(int index, bool left)
    {
        if (index < 0 || index >= _rows.Count) return;
        var row = _rows[index];
        if (left)
        {
            row.Left = "";
            row.LeftKind = ChangeType.Imaginary;
            DirtyLeft = true;
        }
        else
        {
            row.Right = "";
            row.RightKind = ChangeType.Imaginary;
            DirtyRight = true;
        }
        RecalcRow(row);
        if (row.LeftKind == ChangeType.Imaginary && row.RightKind == ChangeType.Imaginary)
            _rows.RemoveAt(index);
    }

    void MoveEdit(int dir, int caret)
    {
        if (_rows.Count == 0) return;
        CommitEdit();
        Select(_selected + dir);
        BeginEdit(_focusLeft, caret);
    }

    void ApplyBoxToRow(int index, bool left, string text)
    {
        if (index < 0 || index >= _rows.Count) return;
        var row = _rows[index];
        if (left)
        {
            row.Left = text;
            if (row.LeftKind == ChangeType.Imaginary) row.LeftKind = ChangeType.Deleted;
            DirtyLeft = true;
        }
        else
        {
            row.Right = text;
            if (row.RightKind == ChangeType.Imaginary) row.RightKind = ChangeType.Inserted;
            DirtyRight = true;
        }
        RecalcRow(row);
    }

    void AfterEdit(bool toRight)
    {
        if (toRight) DirtyRight = true;
        else DirtyLeft = true;
        Reindex();
        UpdateScroll();
        Invalidate();
        Changed?.Invoke();
    }

    static void ApplyCopy(Row row, bool toRight)
    {
        if (toRight)
        {
            if (row.LeftKind == ChangeType.Imaginary)
            {
                row.Right = "";
                row.RightKind = ChangeType.Imaginary;
            }
            else
            {
                row.Right = row.Left;
                row.RightKind = ChangeType.Unchanged;
                row.LeftKind = ChangeType.Unchanged;
            }
        }
        else if (row.RightKind == ChangeType.Imaginary)
        {
            row.Left = "";
            row.LeftKind = ChangeType.Imaginary;
        }
        else
        {
            row.Left = row.Right;
            row.LeftKind = ChangeType.Unchanged;
            row.RightKind = ChangeType.Unchanged;
        }
    }

    (int From, int To) HunkRange(int index)
    {
        if (!_rows[index].IsDiff) return (index, index);
        var from = index;
        var to = index;
        while (from > 0 && _rows[from - 1].IsDiff) from--;
        while (to + 1 < _rows.Count && _rows[to + 1].IsDiff) to++;
        return (from, to);
    }

    void Reindex()
    {
        _leftNo = new int[_rows.Count];
        _rightNo = new int[_rows.Count];
        var ln = 1;
        var rn = 1;
        for (var i = 0; i < _rows.Count; i++)
        {
            if (_rows[i].LeftKind != ChangeType.Imaginary) _leftNo[i] = ln++;
            if (_rows[i].RightKind != ChangeType.Imaginary) _rightNo[i] = rn++;
        }
        _maxChars = 8;
        foreach (var r in _rows)
            _maxChars = Math.Max(_maxChars, Math.Max(r.Left.Length, r.Right.Length));
    }

    void RecalcRow(Row r)
    {
        var leftOn = r.LeftKind != ChangeType.Imaginary;
        var rightOn = r.RightKind != ChangeType.Imaginary;
        if (!leftOn && !rightOn) return;
        if (leftOn && rightOn)
        {
            if (r.Left == r.Right) r.LeftKind = r.RightKind = ChangeType.Unchanged;
            else r.LeftKind = r.RightKind = ChangeType.Modified;
        }
        else if (leftOn)
        {
            r.LeftKind = ChangeType.Deleted;
            r.RightKind = ChangeType.Imaginary;
        }
        else
        {
            r.RightKind = ChangeType.Inserted;
            r.LeftKind = ChangeType.Imaginary;
        }
    }

    void CommitEdit(bool apply = true)
    {
        if (_edit is null || _editIndex < 0) return;
        var box = _edit;
        var idx = _editIndex;
        var left = _editLeft;
        var text = box.Text;
        _editIndex = -1;
        _suspendLostFocus = true;
        try
        {
            box.Hide();
        }
        catch { /* ignore */ }
        _suspendLostFocus = false;
        if (!apply || idx < 0 || idx >= _rows.Count)
        {
            Invalidate();
            return;
        }
        ApplyBoxToRow(idx, left, text);
        Reindex();
        UpdateScroll();
        Invalidate();
        Changed?.Invoke();
    }

    void CancelEdit()
    {
        CommitEdit(false);
        try { Focus(); } catch { /* ignore */ }
    }

    void PlaceEdit()
    {
        if (_edit is null || _editIndex < 0 || _editIndex >= _rows.Count) return;
        var y = RowY(_editIndex);
        var view = ViewRect();
        if (y < -RowHeight || y > view.Height)
        {
            _edit.Hide();
            return;
        }
        var cell = _editLeft ? LeftTextRect() : RightTextRect();
        _edit.Bounds = new Rectangle(cell.X, y, Math.Max(40, cell.Width), RowHeight);
        _edit.Show();
        _edit.BringToFront();
    }

    Rectangle ViewRect()
    {
        var w = Math.Max(1, Width - _v.Width);
        var h = Math.Max(1, Height - _h.Height);
        return new Rectangle(0, 0, w, h);
    }

    int MidX()
    {
        var view = ViewRect();
        return Math.Max(1, view.Width / 2);
    }

    Rectangle LeftPane() => new(0, 0, MidX() - SplitGap / 2, ViewRect().Height);
    Rectangle RightPane()
    {
        var mid = MidX() + SplitGap / 2;
        return new Rectangle(mid, 0, Math.Max(1, ViewRect().Width - mid), ViewRect().Height);
    }

    Rectangle LeftTextRect()
    {
        var p = LeftPane();
        var x = p.X + Gutter + LineNoW - _h.Value;
        return new Rectangle(x, 0, Math.Max(20, p.Right - 4 - x), p.Height);
    }

    Rectangle RightTextRect()
    {
        var p = RightPane();
        var x = p.X + Gutter + LineNoW - _h.Value;
        return new Rectangle(x, 0, Math.Max(20, p.Right - 4 - x), p.Height);
    }

    int RowY(int index) => (index * RowHeight) - _v.Value;

    void LayoutBars()
    {
        var vw = Math.Max(16, _v.Width);
        var hh = Math.Max(16, _h.Height);
        _v.Bounds = new Rectangle(Math.Max(0, Width - vw), 0, vw, Math.Max(1, Height - hh));
        _h.Bounds = new Rectangle(0, Math.Max(0, Height - hh), Math.Max(1, Width - vw), hh);
    }

    void UpdateScroll()
    {
        try
        {
            var view = ViewRect();
            var contentH = Math.Max(view.Height, Math.Max(1, _rows.Count) * RowHeight);
            _v.Minimum = 0;
            _v.SmallChange = RowHeight;
            _v.LargeChange = Math.Max(1, view.Height);
            _v.Maximum = Math.Max(0, contentH - 1);
            SetV(_v.Value);

            var paneW = Math.Max(40, MidX() - Gutter - LineNoW - 8);
            var contentW = Math.Max(paneW, _maxChars * 8 + 16);
            _h.Minimum = 0;
            _h.SmallChange = 16;
            _h.LargeChange = Math.Max(1, paneW);
            _h.Maximum = Math.Max(0, contentW - 1);
            SetH(_h.Value);
        }
        catch { /* 句柄未就绪 */ }
    }

    void SetV(int value)
    {
        try
        {
            if (_v.IsDisposed) return;
            var usable = Math.Max(_v.Minimum, _v.Maximum - Math.Max(1, _v.LargeChange) + 1);
            var v = Math.Clamp(value, _v.Minimum, usable);
            if (_v.Value != v) _v.Value = v;
        }
        catch { /* ignore */ }
    }

    void SetH(int value)
    {
        try
        {
            if (_h.IsDisposed) return;
            var usable = Math.Max(_h.Minimum, _h.Maximum - Math.Max(1, _h.LargeChange) + 1);
            var v = Math.Clamp(value, _h.Minimum, usable);
            if (_h.Value != v) _h.Value = v;
        }
        catch { /* ignore */ }
    }

    void Select(int index)
    {
        if (_rows.Count == 0) return;
        _selected = Math.Clamp(index, 0, _rows.Count - 1);
        EnsureVisible(_selected);
        Invalidate();
    }

    void EnsureVisible(int index)
    {
        var top = index * RowHeight;
        var view = ViewRect();
        if (top < _v.Value) SetV(top);
        else if (top + RowHeight > _v.Value + view.Height)
            SetV(top + RowHeight - view.Height);
    }

    protected override void OnResize(EventArgs e)
    {
        base.OnResize(e);
        LayoutBars();
        UpdateScroll();
        PlaceEdit();
    }

    protected override void OnPaint(PaintEventArgs e)
    {
        try
        {
            PaintRows(e.Graphics);
        }
        catch
        {
            try { e.Graphics.Clear(Theme.Bg); } catch { /* ignore */ }
        }
    }

    void PaintRows(Graphics g)
    {
        g.Clear(Theme.Bg);
        var view = ViewRect();
        using var midPen = new Pen(Theme.LineStrong);
        g.DrawLine(midPen, MidX(), 0, MidX(), view.Height);

        if (_rows.Count == 0)
        {
            TextRenderer.DrawText(g, "没有可显示的文本", Theme.Ui, view, Theme.Muted,
                TextFormatFlags.HorizontalCenter | TextFormatFlags.VerticalCenter);
            return;
        }

        var first = Math.Max(0, _v.Value / RowHeight);
        var last = Math.Min(_rows.Count - 1, first + view.Height / RowHeight + 2);
        var leftPane = LeftPane();
        var rightPane = RightPane();
        for (var i = first; i <= last; i++)
        {
            var y = RowY(i);
            var rc = new Rectangle(0, y, view.Width, RowHeight);
            var row = _rows[i];
            if (i == _selected)
                using (var br = new SolidBrush(Theme.Selected)) g.FillRectangle(br, rc);
            else if (i == _hover)
                using (var br = new SolidBrush(Theme.Hover)) g.FillRectangle(br, rc);

            PaintSide(g, row.Left, row.LeftKind, _leftNo[i], leftPane, y, i == _selected);
            PaintSide(g, row.Right, row.RightKind, _rightNo[i], rightPane, y, i == _selected);
        }
    }

    void PaintSide(Graphics g, string text, ChangeType kind, int lineNo, Rectangle pane, int y, bool selected)
    {
        var bg = kind switch
        {
            ChangeType.Inserted => Theme.InsertBg,
            ChangeType.Deleted => Theme.DeleteBg,
            ChangeType.Modified => Theme.DiffBg,
            ChangeType.Imaginary => Color.FromArgb(18, 20, 26),
            _ => Color.Empty
        };
        if (bg.A > 0 && !selected)
            using (var br = new SolidBrush(bg))
                g.FillRectangle(br, new Rectangle(pane.X, y, pane.Width, RowHeight));

        var bar = kind switch
        {
            ChangeType.Inserted => Theme.Ok,
            ChangeType.Deleted => Theme.Danger,
            ChangeType.Modified => Theme.Warn,
            _ => Color.Empty
        };
        if (bar.A > 0)
            using (var br = new SolidBrush(bar))
                g.FillRectangle(br, new Rectangle(pane.X, y, 3, RowHeight));

        var noRect = new Rectangle(pane.X + Gutter, y, LineNoW - 6, RowHeight);
        if (kind != ChangeType.Imaginary && lineNo > 0)
            TextRenderer.DrawText(g, lineNo.ToString(), Theme.Small, noRect, Theme.Faint,
                TextFormatFlags.Right | TextFormatFlags.VerticalCenter | TextFormatFlags.NoPadding);

        var tx = pane.X + Gutter + LineNoW - _h.Value;
        var textRect = new Rectangle(tx, y, Math.Max(8, pane.Right - 4 - tx), RowHeight);
        var fg = kind switch
        {
            ChangeType.Inserted => Theme.Ok,
            ChangeType.Deleted => Theme.Danger,
            ChangeType.Modified => Theme.Warn,
            ChangeType.Imaginary => Theme.Faint,
            _ => Theme.Text
        };
        var hideText = _edit is not null && _edit.Visible && _editIndex == _selected && selected &&
                       ((_editLeft && pane.X == LeftPane().X) || (!_editLeft && pane.X == RightPane().X));
        var shown = kind == ChangeType.Imaginary || hideText ? "" : text;
        TextRenderer.DrawText(g, shown, Theme.Mono, textRect, fg,
            TextFormatFlags.Left | TextFormatFlags.VerticalCenter | TextFormatFlags.NoPrefix | TextFormatFlags.EndEllipsis | TextFormatFlags.NoPadding);
    }

    protected override void OnMouseMove(MouseEventArgs e)
    {
        var i = HitRow(e.Y);
        if (i != _hover) { _hover = i; Invalidate(); }
        Cursor = i >= 0 ? Cursors.IBeam : Cursors.Default;
    }

    protected override void OnMouseLeave(EventArgs e)
    {
        _hover = -1;
        Cursor = Cursors.Default;
        Invalidate();
    }

    protected override void OnMouseDown(MouseEventArgs e)
    {
        Focus();
        if (e.Button != MouseButtons.Left && e.Button != MouseButtons.Right) return;
        var i = HitRow(e.Y);
        if (i < 0)
        {
            if (_rows.Count == 0)
            {
                _rows.Add(new Row { LeftKind = ChangeType.Unchanged, RightKind = ChangeType.Unchanged });
                Reindex();
                i = 0;
            }
            else return;
        }
        _focusLeft = e.X < MidX();
        Select(i);
        if (e.Button == MouseButtons.Left)
        {
            BeginEdit(_focusLeft);
            if (_edit is not null)
            {
                var pt = _edit.PointToClient(PointToScreen(e.Location));
                _edit.SelectionStart = _edit.GetCharIndexFromPosition(pt);
                _edit.SelectionLength = 0;
            }
        }
    }

    protected override void OnMouseDoubleClick(MouseEventArgs e)
    {
        var i = HitRow(e.Y);
        if (i < 0) return;
        Select(i);
        BeginEdit(e.X < MidX(), selectAll: true);
    }

    int HitRow(int y)
    {
        if (_rows.Count == 0) return -1;
        var i = (_v.Value + y) / RowHeight;
        return i >= 0 && i < _rows.Count ? i : -1;
    }

    protected override bool ProcessCmdKey(ref Message msg, Keys keyData)
    {
        if (_edit is not null && _edit.Visible && _edit.Focused)
            return base.ProcessCmdKey(ref msg, keyData);
        switch (keyData)
        {
            case Keys.Up:
                Select(_selected - 1); BeginEdit(_focusLeft); return true;
            case Keys.Down:
                Select(_selected + 1); BeginEdit(_focusLeft); return true;
            case Keys.PageUp:
                Select(_selected - Math.Max(1, ViewRect().Height / RowHeight)); BeginEdit(_focusLeft); return true;
            case Keys.PageDown:
                Select(_selected + Math.Max(1, ViewRect().Height / RowHeight)); BeginEdit(_focusLeft); return true;
            case Keys.Home:
                Select(0); BeginEdit(_focusLeft, 0); return true;
            case Keys.End:
                Select(_rows.Count - 1); BeginEdit(_focusLeft); return true;
            case Keys.F2:
            case Keys.Enter:
                BeginEdit(_focusLeft, selectAll: true); return true;
            case Keys.Control | Keys.Down:
                Jump(1); return true;
            case Keys.Control | Keys.Up:
                Jump(-1); return true;
            case Keys.Control | Keys.Right:
                CopyLine(true); return true;
            case Keys.Control | Keys.Left:
                CopyLine(false); return true;
            case Keys.Control | Keys.Shift | Keys.Right:
                CopyHunk(true); return true;
            case Keys.Control | Keys.Shift | Keys.Left:
                CopyHunk(false); return true;
        }
        return base.ProcessCmdKey(ref msg, keyData);
    }

    ContextMenuStrip BuildMenu()
    {
        var m = new ContextMenuStrip();
        m.Opening += (_, e) =>
        {
            m.Items.Clear();
            if (_rows.Count == 0) { e.Cancel = true; return; }
            m.Items.Add("编辑左侧", null, (_, _) => BeginEdit(true));
            m.Items.Add("编辑右侧", null, (_, _) => BeginEdit(false));
            m.Items.Add("插入行", null, (_, _) =>
            {
                BeginEdit(_focusLeft);
                SplitLine();
            });
            m.Items.Add("删除本行", null, (_, _) =>
            {
                CommitEdit();
                RemoveSide(_selected, _focusLeft);
                Reindex();
                UpdateScroll();
                Invalidate();
                Changed?.Invoke();
            });
            m.Items.Add(new ToolStripSeparator());
            m.Items.Add("复制本行到右侧", null, (_, _) => CopyLine(true));
            m.Items.Add("复制本行到左侧", null, (_, _) => CopyLine(false));
            m.Items.Add("复制差异块到右侧", null, (_, _) => CopyHunk(true));
            m.Items.Add("复制差异块到左侧", null, (_, _) => CopyHunk(false));
        };
        return m;
    }
}
