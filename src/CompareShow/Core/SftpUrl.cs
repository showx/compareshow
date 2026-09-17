using System.Text.RegularExpressions;

namespace CompareShow.Core;

public sealed class ParsedSftpUrl
{
    public string? Username { get; set; }
    public string? Password { get; set; }
    public string Host { get; set; } = "";
    public int? Port { get; set; }
    public string Path { get; set; } = "/";
    public string Scheme { get; set; } = "sftp";
}

public static class SftpUrl
{
    public static bool LooksLikeSftpInput(string raw)
    {
        var text = raw.Trim();
        if (text.Length == 0) return false;
        if (Regex.IsMatch(text, @"^(sftp|scp|ssh)://", RegexOptions.IgnoreCase)) return true;
        if (Regex.IsMatch(text, @"^[a-zA-Z]:[\\/]") || text.StartsWith(@"\\") || text.StartsWith('/')) return false;
        return Regex.IsMatch(text, @"^[^\s@/\\]+@[^@\s/\\]+");
    }

    public static ParsedSftpUrl? Parse(string raw)
    {
        var text = raw.Trim();
        if (text.Length == 0) return null;

        var schemeMatch = Regex.Match(text, @"^(sftp|scp|ssh)://", RegexOptions.IgnoreCase);
        var scheme = schemeMatch.Success ? schemeMatch.Groups[1].Value.ToLowerInvariant() : "sftp";
        var rest = schemeMatch.Success ? text[schemeMatch.Length..] : text;
        if (!schemeMatch.Success && !LooksLikeSftpInput(text)) return null;

        string username = "";
        string? password = null;
        var hostportpath = rest;

        var at = rest.LastIndexOf('@');
        if (at >= 0)
        {
            var userinfo = rest[..at];
            hostportpath = rest[(at + 1)..];
            var colon = userinfo.IndexOf(':');
            if (colon >= 0)
            {
                username = Decode(userinfo[..colon]);
                password = Decode(userinfo[(colon + 1)..]);
            }
            else username = Decode(userinfo);
        }

        var path = "/";
        var hostport = hostportpath;
        var slash = hostportpath.IndexOf('/');
        if (slash >= 0)
        {
            hostport = hostportpath[..slash];
            path = hostportpath[slash..];
            if (path.Length == 0) path = "/";
        }
        else
        {
            var scp = Regex.Match(hostportpath, @"^([^:]+):([^/].*)$");
            if (scp.Success && !Regex.IsMatch(scp.Groups[2].Value, @"^\d+$"))
            {
                hostport = scp.Groups[1].Value;
                var p = scp.Groups[2].Value;
                path = p.StartsWith('/') ? p : "/" + p;
            }
        }

        string host;
        int? port = null;
        var portMatch = Regex.Match(hostport, @"^(.+):(\d+)$");
        if (portMatch.Success)
        {
            host = portMatch.Groups[1].Value;
            port = int.TryParse(portMatch.Groups[2].Value, out var n) ? n : 22;
        }
        else host = hostport;

        host = host.Trim().TrimStart('[').TrimEnd(']').Trim();
        if (host.Length == 0) return null;
        if (!path.StartsWith('/')) path = "/" + path;
        if (path.Length > 1) path = path.TrimEnd('/');
        if (path.Length == 0) path = "/";

        return new ParsedSftpUrl
        {
            Username = string.IsNullOrEmpty(username) ? null : username,
            Password = password,
            Host = host,
            Port = port,
            Path = path,
            Scheme = scheme
        };
    }

    public static string Format(string? username, string host, int? port, string path, string? scheme = null)
    {
        var sch = scheme == "ssh" ? "ssh" : "sftp";
        var user = string.IsNullOrEmpty(username) ? "" : username + "@";
        var p = port is > 0 and not 22 ? $":{port}" : "";
        return $"{sch}://{user}{host}{p}{path}";
    }

    static string Decode(string value)
    {
        try { return Uri.UnescapeDataString(value); }
        catch { return value; }
    }
}
