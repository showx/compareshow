namespace CompareShow.Core;

public enum LocationKind { Local, Sftp }
public enum AuthType { Password, PrivateKey }
public enum CompareMode { Quick, Content }
public enum ItemStatus { Same, Different, LeftOnly, RightOnly, Conflict, Unknown }
public enum EntryType { File, Dir }

public sealed class SftpConnectionConfig
{
    public string Id { get; set; } = "";
    public string Name { get; set; } = "";
    public string Host { get; set; } = "";
    public int Port { get; set; } = 22;
    public string Username { get; set; } = "";
    public AuthType AuthType { get; set; } = AuthType.Password;
    public string? Password { get; set; }
    public string? PrivateKeyPath { get; set; }
    public string? Passphrase { get; set; }
    public string? DefaultPath { get; set; } = "/";
    public long CreatedAt { get; set; }
    public long LastUsedAt { get; set; }

    public SftpConnectionConfig Clone() => (SftpConnectionConfig)MemberwiseClone();
}

public sealed class LocationRef
{
    public LocationKind Kind { get; set; } = LocationKind.Local;
    public string Path { get; set; } = "";
    public string? ConnectionId { get; set; }
    public string? DisplayUrl { get; set; }

    public LocationRef Clone() => new()
    {
        Kind = Kind,
        Path = Path,
        ConnectionId = ConnectionId,
        DisplayUrl = DisplayUrl
    };
}

public sealed class FileMeta
{
    public string Name { get; set; } = "";
    public string Path { get; set; } = "";
    public EntryType Type { get; set; }
    public long Size { get; set; }
    public long Mtime { get; set; }
    public int? Mode { get; set; }
}

public sealed class CompareNode
{
    public string Name { get; set; } = "";
    public string RelPath { get; set; } = "";
    public EntryType Type { get; set; }
    public ItemStatus Status { get; set; } = ItemStatus.Unknown;
    public FileMeta? Left { get; set; }
    public FileMeta? Right { get; set; }
    public List<CompareNode>? Children { get; set; }
}

public sealed class CompareStats
{
    public int Same { get; set; }
    public int Different { get; set; }
    public int LeftOnly { get; set; }
    public int RightOnly { get; set; }
    public int Files { get; set; }
    public int Dirs { get; set; }
}

public sealed class FolderCompareResult
{
    public List<CompareNode> Tree { get; set; } = [];
    public CompareStats Stats { get; set; } = new();
}

public sealed class FileCompareResult
{
    public FileMeta? LeftMeta { get; set; }
    public FileMeta? RightMeta { get; set; }
    public string? LeftText { get; set; }
    public string? RightText { get; set; }
    public bool LeftBinary { get; set; }
    public bool RightBinary { get; set; }
    public string? LeftHash { get; set; }
    public string? RightHash { get; set; }
    public bool Identical { get; set; }
    public bool TooLarge { get; set; }
    public string? Encoding { get; set; }
}

public sealed class RecentSession
{
    public string Id { get; set; } = "";
    public string Title { get; set; } = "";
    public LocationKind LeftKind { get; set; }
    public LocationKind RightKind { get; set; }
    public string LeftPath { get; set; } = "";
    public string RightPath { get; set; } = "";
    public string? LeftConnectionId { get; set; }
    public string? RightConnectionId { get; set; }
    public long At { get; set; }
}

public sealed class OpenSftpResult
{
    public bool Ok { get; set; }
    public string? Error { get; set; }
    public bool NeedPassword { get; set; }
    public string? Username { get; set; }
    public string? Host { get; set; }
    public int Port { get; set; } = 22;
    public string[] KeysTried { get; set; } = [];
    public string? ConnectionId { get; set; }
    public string? Path { get; set; }
    public string? DisplayUrl { get; set; }
    public SftpConnectionConfig? Connection { get; set; }
}

public static class AppLimits
{
    public const long TextMaxBytes = 8L * 1024 * 1024;
    public static readonly string[] DefaultIgnore =
        ["node_modules", ".git", ".svn", "dist", "out", ".DS_Store", "Thumbs.db"];
}
