namespace CompareShow.Core;

public static class LocationPaths
{
    public static bool IsWinDrivesRoot(string path) => path is "\\" or "/";

    public static bool LooksLikeLocalPath(string raw)
    {
        var text = raw.Trim().Trim('\'', '"');
        if (text.Length == 0) return false;
        if (text.StartsWith("file:", StringComparison.OrdinalIgnoreCase)) return true;
        if (System.Text.RegularExpressions.Regex.IsMatch(text, @"^[a-zA-Z]:[\\/]?")) return true;
        if (text is "\\" or "此电脑" || text.StartsWith(@"\\")) return true;
        if (text == "~" || text.StartsWith("~/") || text.StartsWith("~\\")) return true;
        if (System.Text.RegularExpressions.Regex.IsMatch(text, @"^%[A-Za-z0-9_]+%")) return true;
        return false;
    }

    public static string NormalizeLocalInput(string input)
    {
        var text = (input ?? "").Trim();
        if (text.Length == 0) return "";
        if ((text.StartsWith('"') && text.EndsWith('"') && text.Length >= 2)
            || (text.StartsWith('\'') && text.EndsWith('\'') && text.Length >= 2))
            text = text[1..^1].Trim();
        if (text.StartsWith("file:", StringComparison.OrdinalIgnoreCase))
        {
            text = System.Text.RegularExpressions.Regex.Replace(text, @"^file:\/\/\/?", "", System.Text.RegularExpressions.RegexOptions.IgnoreCase);
            try { text = Uri.UnescapeDataString(text); } catch { /* keep */ }
            if (System.Text.RegularExpressions.Regex.IsMatch(text, @"^\/[a-zA-Z]:")) text = text[1..];
        }
        if (text == "此电脑") return "\\";
        if (System.Text.RegularExpressions.Regex.IsMatch(text, @"^[a-zA-Z]:$")) return text + "\\";
        if (System.Text.RegularExpressions.Regex.IsMatch(text, @"^[a-zA-Z]:[\\/]") || text.StartsWith(@"\\"))
            return text.Replace('/', '\\');
        return text;
    }

    public static bool LocReady(LocationRef loc)
        => loc.Kind == LocationKind.Sftp ? !string.IsNullOrEmpty(loc.ConnectionId) : !string.IsNullOrEmpty(loc.Path);

    public static string LocationDisplay(LocationRef loc, IEnumerable<SftpConnectionConfig> connections)
    {
        if (loc.Kind != LocationKind.Sftp) return IsWinDrivesRoot(loc.Path) ? "此电脑" : loc.Path;
        if (!string.IsNullOrEmpty(loc.DisplayUrl)) return loc.DisplayUrl!;
        var c = connections.FirstOrDefault(x => x.Id == loc.ConnectionId);
        if (c is null) return loc.Path;
        return SftpUrl.Format(c.Username, c.Host, c.Port, string.IsNullOrEmpty(loc.Path) ? "/" : loc.Path);
    }

    public static LocationRef WithPath(LocationRef loc, string path)
    {
        var copy = loc.Clone();
        copy.Path = path;
        return copy;
    }

    public static LocationRef ParentPath(LocationRef loc)
    {
        if (loc.Kind == LocationKind.Local)
        {
            if (IsWinDrivesRoot(loc.Path)) return loc.Clone();
            var normalized = loc.Path.TrimEnd('\\', '/');
            if (System.Text.RegularExpressions.Regex.IsMatch(normalized, @"^[a-zA-Z]:$"))
                return WithPath(loc, "\\");
            var idx = Math.Max(normalized.LastIndexOf('\\'), normalized.LastIndexOf('/'));
            if (idx <= 0) return loc.Clone();
            var parent = normalized[..idx];
            if (System.Text.RegularExpressions.Regex.IsMatch(parent, @"^[a-zA-Z]:$")) parent += "\\";
            return WithPath(loc, parent);
        }
        var parts = loc.Path.TrimEnd('/').Split('/');
        var next = string.Join('/', parts.Take(Math.Max(0, parts.Length - 1)));
        if (string.IsNullOrEmpty(next)) next = "/";
        var copy = WithPath(loc, next);
        copy.DisplayUrl = null;
        return copy;
    }

    public static LocationRef ChildLoc(LocationRef loc, string name, string? metaPath = null)
    {
        if (!string.IsNullOrEmpty(metaPath)) return WithPath(loc, metaPath);
        if (loc.Kind == LocationKind.Local)
        {
            if (IsWinDrivesRoot(loc.Path))
            {
                if (System.Text.RegularExpressions.Regex.IsMatch(name, @"^[a-zA-Z]:\\?$"))
                    return WithPath(loc, name.TrimEnd('\\') + "\\");
                return loc.Clone();
            }
            return WithPath(loc, LocalFs.Join(loc.Path, name));
        }
        var basePath = loc.Path == "/" ? "" : loc.Path.TrimEnd('/');
        return WithPath(loc, $"{basePath}/{name}");
    }

    public static LocationRef JoinRel(LocationRef loc, string rel)
    {
        if (string.IsNullOrEmpty(rel)) return loc.Clone();
        if (loc.Kind == LocationKind.Local)
        {
            var sep = loc.Path.Contains('\\') ? '\\' : '/';
            return WithPath(loc, loc.Path.TrimEnd('\\', '/') + sep + rel.Replace('/', sep));
        }
        var basePath = loc.Path == "/" ? "" : loc.Path.TrimEnd('/');
        return WithPath(loc, $"{basePath}/{rel}");
    }

    public static bool CanGoUp(LocationRef loc)
    {
        if (string.IsNullOrEmpty(loc.Path)) return false;
        if (loc.Kind == LocationKind.Sftp) return loc.Path != "/";
        if (IsWinDrivesRoot(loc.Path)) return false;
        var n = loc.Path.TrimEnd('\\', '/');
        if (System.Text.RegularExpressions.Regex.IsMatch(n, @"^[a-zA-Z]:$")) return true;
        return n.Contains('\\') || n.Contains('/') || System.Text.RegularExpressions.Regex.IsMatch(loc.Path, @"^[a-zA-Z]:\\.+");
    }

    public static string ShortTitle(LocationRef left, LocationRef right)
    {
        static string Tail(string p)
        {
            var t = p.TrimEnd('\\', '/');
            var parts = t.Split(['\\', '/'], StringSplitOptions.RemoveEmptyEntries);
            return parts.Length == 0 ? (p.Length == 0 ? "文件夹比较" : p) : parts[^1];
        }
        if (string.IsNullOrEmpty(left.Path) && string.IsNullOrEmpty(right.Path)) return "文件夹比较";
        return $"{Tail(left.Path)} ↔ {Tail(right.Path)}";
    }
}
