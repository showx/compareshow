using CompareShow.Core;
using CompareShow.Ui;

namespace CompareShow;

static class Program
{
    [STAThread]
    static void Main(string[] args)
    {
        Application.SetUnhandledExceptionMode(UnhandledExceptionMode.CatchException);
        Application.ThreadException += (_, e) => ShowError(e.Exception);
        AppDomain.CurrentDomain.UnhandledException += (_, e) =>
        {
            if (e.ExceptionObject is Exception ex) ShowError(ex);
        };
        ApplicationConfiguration.Initialize();
        if (args.Contains("--smoke"))
        {
            SmokePaint();
            return;
        }
        if (args.Contains("--list"))
        {
            var dir = args.SkipWhile(a => a != "--list").Skip(1).FirstOrDefault() ?? @"D:\code\ywshow\compareshow";
            var items = LocalFs.List(dir);
            Console.WriteLine(dir + " => " + items.Count);
            foreach (var it in items.Take(30))
                Console.WriteLine((it.Type == EntryType.Dir ? "DIR " : "FILE") + " " + it.Name);
            return;
        }
        using var app = new AppServices();
        Application.Run(new MainForm(app));
    }

    static void SmokePaint()
    {
        using var form = new Form { Size = new Size(1200, 800), Font = Theme.Ui, BackColor = Theme.Bg };
        var list = new CompareListControl { Dock = DockStyle.Fill };
        var diff = new LineDiffControl { Dock = DockStyle.Bottom, Height = 240 };
        form.Controls.Add(list);
        form.Controls.Add(diff);
        var rows = Enumerable.Range(0, 24).Select(i => new CompareListControl.FlatRow
        {
            Depth = 0,
            Node = new CompareNode
            {
                Name = "item" + i,
                RelPath = "item" + i,
                Type = i % 3 == 0 ? EntryType.Dir : EntryType.File,
                Status = ItemStatus.LeftOnly,
                Left = new FileMeta { Name = "item" + i, Size = 1024, Mtime = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds() }
            }
        }).ToList();
        form.Show();
        list.SetRows(rows);
        list.ShowParentRow = true;
        diff.LoadTexts("alpha\nbeta\ngamma\n", "alpha\nBETA\ngamma\nextra\n", false);
        if (!diff.Jump(1)) throw new InvalidOperationException("jump-miss");
        diff.CopyLine(true);
        var rightText = diff.BuildRight().Replace("\r\n", "\n");
        if (!rightText.Contains("beta")) throw new InvalidOperationException("copy-line-miss");
        Application.DoEvents();
        using var bmp = new Bitmap(Math.Max(1, list.Width), Math.Max(1, list.Height));
        list.DrawToBitmap(bmp, new Rectangle(0, 0, bmp.Width, bmp.Height));
        using var bmp2 = new Bitmap(Math.Max(1, diff.Width), Math.Max(1, diff.Height));
        diff.DrawToBitmap(bmp2, new Rectangle(0, 0, bmp2.Width, bmp2.Height));
        Console.WriteLine("smoke-ok");
        form.Close();
    }

    static void ShowError(Exception ex)
    {
        try
        {
            MessageBox.Show(ex.Message, "CompareShow", MessageBoxButtons.OK, MessageBoxIcon.Error);
        }
        catch
        {
            /* ignore */
        }
    }
}
