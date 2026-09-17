using CompareShow.Core;

namespace CompareShow.Ui;

public sealed class RemoteBrowserForm : Form
{
    readonly AppServices _app;
    readonly string _connectionId;
    readonly TextBox _path;
    readonly ListBox _list;
    readonly Label _status;
    List<FileMeta> _items = [];

    public string SelectedPath { get; private set; }

    public RemoteBrowserForm(AppServices app, string connectionId, string startPath)
    {
        _app = app;
        _connectionId = connectionId;
        SelectedPath = startPath;
        Text = "浏览远程目录";
        StartPosition = FormStartPosition.CenterParent;
        Size = new Size(560, 480);
        MinimizeBox = false;
        ShowInTaskbar = false;
        Theme.StyleForm(this);

        _path = Theme.Input();
        _path.Location = new Point(48, 12);
        _path.Width = 390;
        _path.Text = startPath;
        _path.KeyDown += async (_, e) => { if (e.KeyCode == Keys.Enter) await LoadAsync(_path.Text); };

        var up = Theme.Ghost("↑");
        up.Location = new Point(12, 10);
        up.Width = 32;
        up.Click += async (_, _) =>
        {
            if (_path.Text is "" or "/") return;
            var next = string.Join('/', _path.Text.TrimEnd('/').Split('/').SkipLast(1));
            await LoadAsync(string.IsNullOrEmpty(next) ? "/" : next);
        };
        var go = Theme.Ghost("转到");
        go.Location = new Point(446, 10);
        go.Click += async (_, _) => await LoadAsync(_path.Text);

        _list = new ListBox
        {
            Location = new Point(12, 48),
            Size = new Size(520, 340),
            BackColor = Color.FromArgb(18, 22, 30),
            ForeColor = Theme.Text,
            BorderStyle = BorderStyle.FixedSingle,
            Font = Theme.Ui
        };
        _list.DoubleClick += async (_, _) =>
        {
            if (_list.SelectedIndex < 0 || _list.SelectedIndex >= _items.Count) return;
            var it = _items[_list.SelectedIndex];
            if (it.Type == EntryType.Dir) await LoadAsync(it.Path);
        };

        _status = new Label { Location = new Point(12, 396), Size = new Size(300, 22), ForeColor = Theme.Muted };
        var cancel = Theme.Ghost("取消");
        cancel.Location = new Point(340, 408);
        cancel.Click += (_, _) => { DialogResult = DialogResult.Cancel; Close(); };
        var ok = Theme.Primary("选择此目录");
        ok.Location = new Point(430, 408);
        ok.Click += (_, _) =>
        {
            if (_list.SelectedIndex >= 0 && _list.SelectedIndex < _items.Count && _items[_list.SelectedIndex].Type == EntryType.Dir)
                SelectedPath = _items[_list.SelectedIndex].Path;
            else
                SelectedPath = _path.Text;
            DialogResult = DialogResult.OK;
            Close();
        };

        Controls.AddRange(new Control[] { up, _path, go, _list, _status, cancel, ok });
        Shown += async (_, _) => await LoadAsync(startPath);
    }

    async Task LoadAsync(string p)
    {
        _status.ForeColor = Theme.Muted;
        _status.Text = "正在读取…";
        try
        {
            if (!_app.Sftp.HasConnection(_connectionId))
            {
                var cfg = _app.Store.GetConnection(_connectionId) ?? throw new InvalidOperationException("找不到连接");
                await _app.Sftp.ConnectAsync(cfg);
            }
            _items = await _app.Sftp.ListAsync(_connectionId, p);
            _path.Text = p;
            SelectedPath = p;
            _list.BeginUpdate();
            _list.Items.Clear();
            foreach (var it in _items)
                _list.Items.Add((it.Type == EntryType.Dir ? "📁 " : "📄 ") + it.Name);
            _list.EndUpdate();
            _status.Text = $"{_items.Count} 项";
        }
        catch (Exception ex)
        {
            _status.ForeColor = Theme.Danger;
            _status.Text = ex.Message;
        }
    }
}
