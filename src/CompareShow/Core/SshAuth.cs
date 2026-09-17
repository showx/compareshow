using System.Text.RegularExpressions;

namespace CompareShow.Core;

public sealed class ResolvedSsh
{
    public string Host { get; set; } = "";
    public int Port { get; set; } = 22;
    public string Username { get; set; } = "";
    public List<string> IdentityFiles { get; set; } = [];
}

public sealed class SshConnectException : Exception
{
    public string Code { get; }
    public IReadOnlyList<string> KeysTried { get; }

    public SshConnectException(string message, string code, IReadOnlyList<string>? keysTried = null)
        : base(message)
    {
        Code = code;
        KeysTried = keysTried ?? [];
    }
}

public static class SshAuth
{
    public static string SshDir() => Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), ".ssh");

    public static IEnumerable<string> DefaultIdentityFiles()
    {
        var dir = SshDir();
        foreach (var name in new[] { "id_ed25519", "id_ecdsa", "id_rsa", "id_dsa" })
        {
            var p = Path.Combine(dir, name);
            if (File.Exists(p)) yield return p;
        }
    }

    public static ResolvedSsh Resolve(string host, string? username = null, int? port = null)
    {
        var blocks = LoadSshConfig();
        var matched = blocks.Where(b => b.Patterns.Any(p => MatchHost(p, host))).ToList();
        var hostName = matched.Select(b => b.HostName).FirstOrDefault(v => !string.IsNullOrEmpty(v)) ?? host;
        var user = username
            ?? matched.Select(b => b.User).FirstOrDefault(v => !string.IsNullOrEmpty(v))
            ?? Environment.UserName
            ?? "root";
        var resolvedPort = port ?? matched.Select(b => b.Port).FirstOrDefault(v => v is > 0) ?? 22;

        var files = new List<string>();
        var seen = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        foreach (var f in matched.SelectMany(b => b.IdentityFiles).Concat(DefaultIdentityFiles()))
        {
            var expanded = ExpandPath(f);
            if (!File.Exists(expanded) || !seen.Add(expanded)) continue;
            files.Add(expanded);
        }

        return new ResolvedSsh { Host = hostName, Port = resolvedPort, Username = user, IdentityFiles = files };
    }

    public static bool IsAuthFailure(Exception err)
    {
        var m = err.Message.ToLowerInvariant();
        return m.Contains("auth")
            || m.Contains("permission denied")
            || m.Contains("unable to authenticate")
            || m.Contains("all configured authentication methods failed")
            || m.Contains("encrypted private")
            || m.Contains("passphrase")
            || m.Contains("no matching")
            || m.Contains("login")
            || m.Contains("denied");
    }

    public static bool IsHostUnreachable(Exception err)
    {
        var m = err.Message.ToLowerInvariant();
        return err is System.Net.Sockets.SocketException
            || m.Contains("timed out")
            || m.Contains("timeout")
            || m.Contains("refused")
            || m.Contains("unreachable")
            || m.Contains("no such host")
            || m.Contains("could not resolve");
    }

    sealed class HostBlock
    {
        public List<string> Patterns { get; set; } = [];
        public string? HostName { get; set; }
        public string? User { get; set; }
        public int? Port { get; set; }
        public List<string> IdentityFiles { get; set; } = [];
    }

    static List<HostBlock> LoadSshConfig()
    {
        var file = Path.Combine(SshDir(), "config");
        if (!File.Exists(file)) return [];
        string text;
        try { text = File.ReadAllText(file); }
        catch { return []; }

        var blocks = new List<HostBlock>();
        HostBlock? current = null;
        foreach (var raw in text.Split('\n'))
        {
            var line = Regex.Replace(raw, @"#.*$", "").Trim();
            if (line.Length == 0) continue;
            var sep = line.IndexOfAny([' ', '=', '\t']);
            if (sep < 0) continue;
            var key = line[..sep].ToLowerInvariant();
            var value = line[(sep + 1)..].Trim().Trim('"');
            if (key == "host")
            {
                current = new HostBlock { Patterns = value.Split([' ', '\t'], StringSplitOptions.RemoveEmptyEntries).ToList() };
                blocks.Add(current);
                continue;
            }
            if (current is null) continue;
            if (key == "hostname") current.HostName = value;
            else if (key == "user") current.User = value;
            else if (key == "port") current.Port = int.TryParse(value, out var p) ? p : 22;
            else if (key == "identityfile") current.IdentityFiles.Add(value);
        }
        return blocks;
    }

    static bool MatchHost(string pattern, string host)
    {
        var negated = pattern.StartsWith('!');
        var p = negated ? pattern[1..] : pattern;
        var re = "^" + Regex.Escape(p).Replace("\\*", ".*").Replace("\\?", ".") + "$";
        var ok = Regex.IsMatch(host, re, RegexOptions.IgnoreCase);
        return negated ? !ok : ok;
    }

    static string ExpandPath(string p)
    {
        var home = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        if (p == "~") return home;
        if (p.StartsWith("~/") || p.StartsWith("~\\")) return Path.Combine(home, p[2..]);
        return Environment.ExpandEnvironmentVariables(p.Replace("%d", home).Replace("%u", Environment.UserName));
    }
}
