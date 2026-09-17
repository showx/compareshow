namespace CompareShow.Ui;

public static class Theme
{
    public static readonly Color Bg = Color.FromArgb(14, 17, 22);
    public static readonly Color Raise = Color.FromArgb(21, 25, 34);
    public static readonly Color Hover = Color.FromArgb(28, 34, 48);
    public static readonly Color Active = Color.FromArgb(35, 43, 60);
    public static readonly Color Line = Color.FromArgb(42, 49, 64);
    public static readonly Color LineStrong = Color.FromArgb(58, 67, 86);
    public static readonly Color Text = Color.FromArgb(231, 234, 240);
    public static readonly Color Muted = Color.FromArgb(139, 147, 163);
    public static readonly Color Faint = Color.FromArgb(92, 101, 118);
    public static readonly Color Accent = Color.FromArgb(77, 163, 255);
    public static readonly Color Ok = Color.FromArgb(60, 207, 145);
    public static readonly Color Warn = Color.FromArgb(224, 138, 60);
    public static readonly Color Danger = Color.FromArgb(232, 93, 93);
    public static readonly Color Left = Color.FromArgb(90, 167, 255);
    public static readonly Color Right = Color.FromArgb(181, 123, 255);
    public static readonly Color Selected = Color.FromArgb(36, 58, 92);
    public static readonly Color DiffBg = Color.FromArgb(48, 36, 22);
    public static readonly Color LeftOnlyBg = Color.FromArgb(22, 36, 52);
    public static readonly Color RightOnlyBg = Color.FromArgb(36, 26, 48);
    public static readonly Color InsertBg = Color.FromArgb(18, 48, 36);
    public static readonly Color DeleteBg = Color.FromArgb(52, 24, 24);

    public static readonly Font Ui = new("Segoe UI", 9f);
    public static readonly Font UiBold = new("Segoe UI", 9f, FontStyle.Bold);
    public static readonly Font Small = new("Segoe UI", 8.25f);
    public static readonly Font Mono = SafeFont(["Cascadia Code", "Consolas", "Courier New"], 9.75f);

    static Font SafeFont(string[] names, float size)
    {
        foreach (var name in names)
        {
            try
            {
                var f = new Font(name, size, FontStyle.Regular, GraphicsUnit.Point);
                if (string.Equals(f.Name, name, StringComparison.OrdinalIgnoreCase) || f.Name.Length > 0)
                    return f;
            }
            catch { /* try next */ }
        }
        return new Font(FontFamily.GenericMonospace, size);
    }

    public static Button Primary(string text)
    {
        var b = Flat(text);
        b.BackColor = Color.FromArgb(36, 92, 168);
        b.FlatAppearance.BorderColor = Color.FromArgb(64, 130, 214);
        return b;
    }

    public static Button Ghost(string text)
    {
        var b = Flat(text);
        b.BackColor = Raise;
        b.FlatAppearance.BorderColor = LineStrong;
        return b;
    }

    public static Button Flat(string text)
    {
        return new Button
        {
            Text = text,
            FlatStyle = FlatStyle.Flat,
            ForeColor = Text,
            BackColor = Raise,
            Font = Ui,
            Height = 28,
            AutoSize = true,
            AutoSizeMode = AutoSizeMode.GrowAndShrink,
            Padding = new Padding(10, 2, 10, 2),
            Cursor = Cursors.Hand,
            TabStop = false
        };
    }

    public static TextBox Input()
    {
        return new TextBox
        {
            BorderStyle = BorderStyle.FixedSingle,
            BackColor = Color.FromArgb(18, 22, 30),
            ForeColor = Text,
            Font = Ui,
            Height = 28
        };
    }

    public static ComboBox Combo()
    {
        return new ComboBox
        {
            DropDownStyle = ComboBoxStyle.DropDownList,
            FlatStyle = FlatStyle.Flat,
            BackColor = Color.FromArgb(18, 22, 30),
            ForeColor = Text,
            Font = Ui,
            Height = 28
        };
    }

    public static Label Label(string text, bool muted = false)
    {
        return new Label
        {
            Text = text,
            AutoSize = true,
            ForeColor = muted ? Muted : Text,
            Font = Ui,
            BackColor = Color.Transparent
        };
    }

    public static void StyleForm(Form f)
    {
        f.BackColor = Bg;
        f.ForeColor = Text;
        f.Font = Ui;
    }

    public static Color StatusBack(Core.ItemStatus status) => status switch
    {
        Core.ItemStatus.Different => DiffBg,
        Core.ItemStatus.LeftOnly => LeftOnlyBg,
        Core.ItemStatus.RightOnly => RightOnlyBg,
        _ => Bg
    };

    public static Color StatusFore(Core.ItemStatus status, bool missing) => missing ? Faint : status switch
    {
        Core.ItemStatus.Different => Warn,
        Core.ItemStatus.LeftOnly => Left,
        Core.ItemStatus.RightOnly => Right,
        Core.ItemStatus.Same => Muted,
        _ => Text
    };
}
