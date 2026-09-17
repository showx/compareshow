using CompareShow.Core;

namespace CompareShow.Ui;

public sealed class PasswordForm : Form
{
    readonly TextBox _password;
    readonly CheckBox _remember;

    public string Password => _password.Text;
    public bool Remember => _remember.Checked;

    public PasswordForm(string title, string message)
    {
        Text = title;
        FormBorderStyle = FormBorderStyle.FixedDialog;
        StartPosition = FormStartPosition.CenterParent;
        ClientSize = new Size(420, 210);
        MaximizeBox = false;
        MinimizeBox = false;
        ShowInTaskbar = false;
        Theme.StyleForm(this);

        var hint = new Label
        {
            Text = message,
            ForeColor = Theme.Muted,
            Location = new Point(16, 16),
            Size = new Size(388, 52)
        };
        var lab = Theme.Label("服务器密码");
        lab.Location = new Point(16, 76);
        _password = Theme.Input();
        _password.UseSystemPasswordChar = true;
        _password.Location = new Point(16, 98);
        _password.Width = 388;
        _remember = new CheckBox
        {
            Text = "记住密码",
            ForeColor = Theme.Text,
            BackColor = Theme.Bg,
            Checked = true,
            Location = new Point(16, 132),
            AutoSize = true
        };
        var cancel = Theme.Ghost("取消");
        cancel.DialogResult = DialogResult.Cancel;
        cancel.Location = new Point(230, 168);
        var ok = Theme.Primary("登录");
        ok.DialogResult = DialogResult.OK;
        ok.Location = new Point(320, 168);
        AcceptButton = ok;
        CancelButton = cancel;
        Controls.AddRange(new Control[] { hint, lab, _password, _remember, cancel, ok });
        Shown += (_, _) => _password.Focus();
    }
}
