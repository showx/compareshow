using CompareShow.Core;

namespace CompareShow.Ui;

public sealed class ConnectForm : Form
{
    readonly AppServices _app;
    readonly TextBox _name;
    readonly TextBox _host;
    readonly NumericUpDown _port;
    readonly TextBox _user;
    readonly ComboBox _auth;
    readonly TextBox _password;
    readonly TextBox _keyPath;
    readonly TextBox _passphrase;
    readonly TextBox _defaultPath;
    readonly Label _msg;
    readonly Panel _passPanel;
    readonly Panel _keyPanel;
    readonly SftpConnectionConfig _form;

    public ConnectForm(AppServices app, SftpConnectionConfig? initial)
    {
        _app = app;
        _form = initial?.Clone() ?? new SftpConnectionConfig { Port = 22, DefaultPath = "/", AuthType = AuthType.Password };
        Text = string.IsNullOrEmpty(_form.Id) ? "新建 SFTP 连接" : "编辑 SFTP 连接";
        FormBorderStyle = FormBorderStyle.FixedDialog;
        StartPosition = FormStartPosition.CenterParent;
        ClientSize = new Size(460, 470);
        MaximizeBox = false;
        MinimizeBox = false;
        ShowInTaskbar = false;
        Theme.StyleForm(this);

        var hint = new Label
        {
            Text = "会自动使用 ~/.ssh 里的私钥。密码仅在密钥登录失败时才需要。",
            ForeColor = Theme.Muted,
            Location = new Point(16, 12),
            Size = new Size(428, 32)
        };

        _name = Field("连接名称", 48, "例如：生产环境", out var nLab);
        _host = Field("主机", 96, "sftp.example.com", out var hLab);
        _host.Width = 250;
        var pLab = Theme.Label("端口");
        pLab.Location = new Point(286, 96);
        _port = new NumericUpDown
        {
            Minimum = 1,
            Maximum = 65535,
            Value = Math.Clamp(_form.Port <= 0 ? 22 : _form.Port, 1, 65535),
            Location = new Point(286, 116),
            Width = 158,
            BackColor = Color.FromArgb(18, 22, 30),
            ForeColor = Theme.Text,
            BorderStyle = BorderStyle.FixedSingle
        };
        _user = Field("用户名", 152, "", out var uLab);
        _user.Width = 250;
        var aLab = Theme.Label("认证方式");
        aLab.Location = new Point(286, 152);
        _auth = Theme.Combo();
        _auth.Items.AddRange(new object[] { "密码", "私钥" });
        _auth.Location = new Point(286, 172);
        _auth.Width = 158;
        _auth.SelectedIndex = _form.AuthType == AuthType.PrivateKey ? 1 : 0;
        _auth.SelectedIndexChanged += (_, _) => ToggleAuth();

        _passPanel = new Panel { Location = new Point(16, 208), Size = new Size(428, 56), BackColor = Theme.Bg };
        var pwLab = Theme.Label("密码");
        _password = Theme.Input();
        _password.UseSystemPasswordChar = true;
        _password.Location = new Point(0, 20);
        _password.Width = 428;
        _password.Text = _form.Password ?? "";
        _passPanel.Controls.AddRange(new Control[] { pwLab, _password });

        _keyPanel = new Panel { Location = new Point(16, 208), Size = new Size(428, 112), BackColor = Theme.Bg };
        var kLab = Theme.Label("私钥路径");
        _keyPath = Theme.Input();
        _keyPath.Location = new Point(0, 20);
        _keyPath.Width = 340;
        _keyPath.Text = _form.PrivateKeyPath ?? "";
        var browse = Theme.Ghost("浏览");
        browse.Location = new Point(348, 18);
        browse.Click += (_, _) =>
        {
            using var d = new OpenFileDialog { Title = "选择私钥文件" };
            if (d.ShowDialog(this) == DialogResult.OK) _keyPath.Text = d.FileName;
        };
        var phLab = Theme.Label("私钥口令（可选）");
        phLab.Location = new Point(0, 52);
        _passphrase = Theme.Input();
        _passphrase.UseSystemPasswordChar = true;
        _passphrase.Location = new Point(0, 72);
        _passphrase.Width = 428;
        _passphrase.Text = _form.Passphrase ?? "";
        _keyPanel.Controls.AddRange(new Control[] { kLab, _keyPath, browse, phLab, _passphrase });

        _defaultPath = Field("默认远程路径", 328, "/", out var dpLab);
        _defaultPath.Text = string.IsNullOrEmpty(_form.DefaultPath) ? "/" : _form.DefaultPath;
        _name.Text = _form.Name;
        _host.Text = _form.Host;
        _user.Text = _form.Username;

        _msg = new Label { Location = new Point(16, 380), Size = new Size(428, 28), ForeColor = Theme.Ok };

        var test = Theme.Ghost("测试连接");
        test.Location = new Point(16, 420);
        test.Click += async (_, _) => await TestAsync(test);
        var save = Theme.Primary("保存");
        save.Location = new Point(360, 420);
        save.Click += (_, _) => Save();
        var cancel = Theme.Ghost("取消");
        cancel.Location = new Point(270, 420);
        cancel.Click += (_, _) => { DialogResult = DialogResult.Cancel; Close(); };

        Controls.AddRange(new Control[] { hint, nLab, _name, hLab, _host, pLab, _port, uLab, _user, aLab, _auth,
            _passPanel, _keyPanel, dpLab, _defaultPath, _msg, test, save, cancel });
        ToggleAuth();
    }

    TextBox Field(string caption, int y, string placeholder, out Label lab)
    {
        lab = Theme.Label(caption);
        lab.Location = new Point(16, y);
        var box = Theme.Input();
        box.PlaceholderText = placeholder;
        box.Location = new Point(16, y + 20);
        box.Width = 428;
        return box;
    }

    void ToggleAuth()
    {
        var key = _auth.SelectedIndex == 1;
        _keyPanel.Visible = key;
        _passPanel.Visible = !key;
    }

    SftpConnectionConfig Collect()
    {
        var c = _form.Clone();
        if (string.IsNullOrEmpty(c.Id)) c.Id = Guid.NewGuid().ToString("N");
        c.Name = string.IsNullOrWhiteSpace(_name.Text) ? $"{_user.Text}@{_host.Text}" : _name.Text.Trim();
        c.Host = _host.Text.Trim();
        c.Port = (int)_port.Value;
        c.Username = _user.Text.Trim();
        c.AuthType = _auth.SelectedIndex == 1 ? AuthType.PrivateKey : AuthType.Password;
        c.Password = _password.Text;
        c.PrivateKeyPath = _keyPath.Text.Trim();
        c.Passphrase = _passphrase.Text;
        c.DefaultPath = string.IsNullOrWhiteSpace(_defaultPath.Text) ? "/" : _defaultPath.Text.Trim();
        if (c.CreatedAt == 0) c.CreatedAt = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
        c.LastUsedAt = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
        if (c.Password == "********")
        {
            var saved = _app.Store.GetConnection(c.Id);
            c.Password = saved?.Password;
        }
        if (c.Passphrase == "********")
        {
            var saved = _app.Store.GetConnection(c.Id);
            c.Passphrase = saved?.Passphrase;
        }
        return c;
    }

    async Task TestAsync(Button btn)
    {
        var c = Collect();
        if (string.IsNullOrEmpty(c.Host) || string.IsNullOrEmpty(c.Username))
        {
            _msg.ForeColor = Theme.Danger;
            _msg.Text = "请填写主机和用户名";
            return;
        }
        btn.Enabled = false;
        _msg.ForeColor = Theme.Muted;
        _msg.Text = "正在连接…";
        try
        {
            await _app.Sftp.ConnectAsync(c);
            _msg.ForeColor = Theme.Ok;
            _msg.Text = "连接成功";
        }
        catch (Exception ex)
        {
            _msg.ForeColor = Theme.Danger;
            _msg.Text = ex.Message;
        }
        finally { btn.Enabled = true; }
    }

    void Save()
    {
        var c = Collect();
        if (string.IsNullOrEmpty(c.Host) || string.IsNullOrEmpty(c.Username))
        {
            _msg.ForeColor = Theme.Danger;
            _msg.Text = "请填写主机和用户名";
            return;
        }
        _app.Store.Upsert(c);
        DialogResult = DialogResult.OK;
        Close();
    }
}
