using System.Text.RegularExpressions;

namespace CompareShow.Core;

public static class LocalFs
{
    public static bool IsDrivesRoot(string path)
    {
        var t = path.Trim();
        return t is "\\" or "/" || t == @"\\?\";
    }

    public static string Normalize(string input)
    {
        var text = (input ?? "").Trim();
        if (text.Length == 0) return "";
        if ((text.StartsWith('"') && text.EndsWith('"') && text.Length >= 2)
            || (text.StartsWith('\'') && text.EndsWith('\'') && text.Length >= 2))
            text = text[1..^1].Trim();

        if (text.StartsWith("file:", StringComparison.OrdinalIgnoreCase))
        {
            try { text = new Uri(text).LocalPath; }
            catch
            {
                text = Regex.Replace(text, @"^file:\/\/", "", RegexOptions.IgnoreCase);
                text = Regex.Replace(text, @"^\/([a-zA-Z]:)", "$1");
            }
        }

        text = Regex.Replace(text, "%([^%]+)%", m =>
        {
            var v = Environment.GetEnvironmentVariable(m.Groups[1].Value);
            return string.IsNullOrEmpty(v) ? m.Value : v;
        });

        var home = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        if (text == "此电脑") return "\\";
        if (text == "~") text = home;
        else if (text.StartsWith("~/") || text.StartsWith("~\\")) text = Path.Combine(home, text[2..]);

        if (text is "/" or "\\") return "\\";
        if (text.StartsWith(@"\\") || text.StartsWith("//"))
            return Path.GetFullPath(text.Replace('/', '\\'));
        text = text.Replace('/', '\\');
        if (Regex.IsMatch(text, @"^[a-zA-Z]:$")) text += "\\";
        try { return Path.GetFullPath(text); }
        catch { return text; }
    }

    public static (string Path, string Type) Resolve(string input)
    {
        var path = Normalize(input);
        if (string.IsNullOrEmpty(path) || IsDrivesRoot(path)) return ("\\", "drives");
        if (!Directory.Exists(path) && !File.Exists(path))
            throw Friendly(new FileNotFoundException(), path);
        if (Directory.Exists(path)) return (path, "dir");
        return (Path.GetDirectoryName(path) ?? path, "file");
    }

    public static List<FileMeta> List(string dir)
    {
        var resolved = Normalize(dir);
        if (IsDrivesRoot(resolved) || string.IsNullOrEmpty(resolved)) return ListDrives();
        if (!Directory.Exists(resolved))
        {
            if (File.Exists(resolved)) throw new InvalidOperationException($"不是文件夹：{resolved}");
            throw new DirectoryNotFoundException($"找不到路径：{resolved}");
        }

        string[] entries;
        try { entries = Directory.GetFileSystemEntries(resolved); }
        catch (Exception ex) { throw Friendly(ex, resolved); }

        var items = new List<FileMeta>(entries.Length);
        foreach (var full in entries)
        {
            var name = Path.GetFileName(full);
            if (string.IsNullOrEmpty(name) || name is "." or "..") continue;
            try
            {
                var attrs = File.GetAttributes(full);
                var isDir = (attrs & FileAttributes.Directory) != 0;
                var mtime = 0L;
                long size = 0;
                if (!isDir)
                {
                    try
                    {
                        var info = new FileInfo(full);
                        size = info.Length;
                        mtime = TextUtil.ToUnixMs(info.LastWriteTime);
                    }
                    catch { /* 没有权限时仍显示名称 */ }
                }
                items.Add(new FileMeta
                {
                    Name = name,
                    Path = full,
                    Type = isDir ? EntryType.Dir : EntryType.File,
                    Size = size,
                    Mtime = mtime
                });
            }
            catch
            {
                items.Add(new FileMeta { Name = name, Path = full, Type = EntryType.File });
            }
        }

        items.Sort(CompareMeta);
        return items;
    }

    public static FileMeta Stat(string path)
    {
        var resolved = Normalize(path);
        if (Directory.Exists(resolved))
        {
            var d = new DirectoryInfo(resolved);
            return new FileMeta { Name = d.Name, Path = resolved, Type = EntryType.Dir, Mtime = TextUtil.ToUnixMs(d.LastWriteTime) };
        }
        var f = new FileInfo(resolved);
        if (!f.Exists) throw new FileNotFoundException($"找不到路径：{resolved}");
        return new FileMeta
        {
            Name = f.Name,
            Path = resolved,
            Type = EntryType.File,
            Size = f.Length,
            Mtime = TextUtil.ToUnixMs(f.LastWriteTime)
        };
    }

    public static byte[] Read(string path, long? maxBytes = null)
    {
        var resolved = Normalize(path);
        var info = new FileInfo(resolved);
        if (maxBytes is > 0 && info.Length > maxBytes) throw new InvalidOperationException("FILE_TOO_LARGE");
        return File.ReadAllBytes(resolved);
    }

    public static void Write(string path, byte[] data)
    {
        var resolved = Normalize(path);
        Directory.CreateDirectory(Path.GetDirectoryName(resolved)!);
        File.WriteAllBytes(resolved, data);
    }

    public static void Mkdir(string path) => Directory.CreateDirectory(Normalize(path));

    public static void Remove(string path)
    {
        var resolved = Normalize(path);
        if (Directory.Exists(resolved)) Directory.Delete(resolved, true);
        else if (File.Exists(resolved)) File.Delete(resolved);
    }

    public static bool Exists(string path)
    {
        var resolved = Normalize(path);
        if (IsDrivesRoot(resolved)) return true;
        return Directory.Exists(resolved) || File.Exists(resolved);
    }

    public static void CopyFile(string src, string dest)
    {
        var from = Normalize(src);
        var to = Normalize(dest);
        Directory.CreateDirectory(Path.GetDirectoryName(to)!);
        File.Copy(from, to, true);
    }

    public static string Join(string dir, string name)
    {
        if (IsDrivesRoot(dir))
        {
            if (Regex.IsMatch(name, @"^[a-zA-Z]:\\?$")) return name.TrimEnd('\\') + "\\";
            return Normalize(name);
        }
        return Path.Combine(dir, name);
    }

    public static string PosixJoin(string dir, string name)
    {
        if (string.IsNullOrEmpty(dir) || dir == "/") return "/" + name;
        return dir.Replace('\\', '/').TrimEnd('/') + "/" + name;
    }

    public static string HashFile(string path)
    {
        using var stream = File.OpenRead(Normalize(path));
        return TextUtil.HashStream(stream);
    }

    static List<FileMeta> ListDrives()
    {
        var names = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        try
        {
            foreach (var d in Environment.GetLogicalDrives())
                names.Add(d.EndsWith('\\') ? d : d + "\\");
        }
        catch { /* ignore */ }
        if (names.Count == 0) names.Add("C:\\");
        return names.OrderBy(x => x, StringComparer.OrdinalIgnoreCase)
            .Select(p => new FileMeta { Name = p[..2], Path = p, Type = EntryType.Dir })
            .ToList();
    }

    static int CompareMeta(FileMeta a, FileMeta b)
    {
        if (a.Type != b.Type) return a.Type == EntryType.Dir ? -1 : 1;
        return string.Compare(a.Name, b.Name, TextUtil.Zh, System.Globalization.CompareOptions.IgnoreCase);
    }

    static Exception Friendly(Exception err, string path)
    {
        return err switch
        {
            FileNotFoundException or DirectoryNotFoundException => new DirectoryNotFoundException($"找不到路径：{path}"),
            UnauthorizedAccessException => new UnauthorizedAccessException($"没有权限访问：{path}"),
            IOException io when io.Message.Contains("being used") => new IOException($"路径正被占用：{path}"),
            _ => err
        };
    }
}
