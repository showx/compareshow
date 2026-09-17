using System.Text.Json;
using System.Text.Json.Serialization;
using System.Security.Cryptography;

namespace CompareShow.Core;

public sealed class SessionStore
{
    readonly object _gate = new();
    readonly JsonSerializerOptions _json = new()
    {
        WriteIndented = true,
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull
    };

    public string StorePath { get; }

    public SessionStore()
    {
        var dir = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "CompareShow");
        Directory.CreateDirectory(dir);
        StorePath = Path.Combine(dir, "sessions.json");
    }

    public List<SftpConnectionConfig> ListConnections()
    {
        lock (_gate) return LoadRaw().Connections.Select(ToPublic).ToList();
    }

    public SftpConnectionConfig Upsert(SftpConnectionConfig input)
    {
        lock (_gate)
        {
            var data = LoadRaw();
            var stored = ToStored(input);
            var idx = data.Connections.FindIndex(c => c.Id == input.Id);
            if (idx >= 0) data.Connections[idx] = stored;
            else data.Connections.Insert(0, stored);
            SaveRaw(data);
            return ToPublic(stored);
        }
    }

    public void DeleteConnection(string id)
    {
        lock (_gate)
        {
            var data = LoadRaw();
            data.Connections.RemoveAll(c => c.Id == id);
            SaveRaw(data);
        }
    }

    public SftpConnectionConfig? GetConnection(string id)
    {
        lock (_gate)
        {
            var found = LoadRaw().Connections.FirstOrDefault(c => c.Id == id);
            return found is null ? null : ToPublic(found);
        }
    }

    public SftpConnectionConfig? FindByEndpoint(string username, string host, int port)
    {
        lock (_gate)
        {
            var found = LoadRaw().Connections.FirstOrDefault(c =>
                string.Equals(c.Username, username, StringComparison.OrdinalIgnoreCase)
                && string.Equals(c.Host, host, StringComparison.OrdinalIgnoreCase)
                && (c.Port == 0 ? 22 : c.Port) == (port == 0 ? 22 : port));
            return found is null ? null : ToPublic(found);
        }
    }

    public void Touch(string id)
    {
        lock (_gate)
        {
            var data = LoadRaw();
            var found = data.Connections.FirstOrDefault(c => c.Id == id);
            if (found is null) return;
            found.LastUsedAt = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
            SaveRaw(data);
        }
    }

    public List<RecentSession> ListRecent()
    {
        lock (_gate) return LoadRaw().Recent.Take(20).ToList();
    }

    public void AddRecent(RecentSession session)
    {
        lock (_gate)
        {
            var data = LoadRaw();
            data.Recent = new[] { session }
                .Concat(data.Recent.Where(r =>
                    !(r.LeftPath == session.LeftPath
                      && r.RightPath == session.RightPath
                      && r.LeftConnectionId == session.LeftConnectionId
                      && r.RightConnectionId == session.RightConnectionId)))
                .Take(20)
                .ToList();
            SaveRaw(data);
        }
    }

    StoreShape LoadRaw()
    {
        try
        {
            var json = File.ReadAllText(StorePath);
            return JsonSerializer.Deserialize<StoreShape>(json, _json) ?? new StoreShape();
        }
        catch { return new StoreShape(); }
    }

    void SaveRaw(StoreShape data) => File.WriteAllText(StorePath, JsonSerializer.Serialize(data, _json));

    static StoredConnection ToStored(SftpConnectionConfig input) => new()
    {
        Id = input.Id,
        Name = input.Name,
        Host = input.Host,
        Port = input.Port,
        Username = input.Username,
        AuthType = input.AuthType.ToString(),
        PrivateKeyPath = input.PrivateKeyPath,
        DefaultPath = input.DefaultPath,
        CreatedAt = input.CreatedAt,
        LastUsedAt = input.LastUsedAt,
        PasswordEnc = Encrypt(input.Password),
        PassphraseEnc = Encrypt(input.Passphrase)
    };

    static SftpConnectionConfig ToPublic(StoredConnection s) => new()
    {
        Id = s.Id,
        Name = s.Name,
        Host = s.Host,
        Port = s.Port,
        Username = s.Username,
        AuthType = Enum.TryParse<AuthType>(s.AuthType, true, out var a) ? a : AuthType.Password,
        PrivateKeyPath = s.PrivateKeyPath,
        DefaultPath = s.DefaultPath,
        CreatedAt = s.CreatedAt,
        LastUsedAt = s.LastUsedAt,
        Password = Decrypt(s.PasswordEnc),
        Passphrase = Decrypt(s.PassphraseEnc)
    };

    static string? Encrypt(string? value)
    {
        if (string.IsNullOrEmpty(value)) return null;
        var bytes = ProtectedData.Protect(System.Text.Encoding.UTF8.GetBytes(value), null, DataProtectionScope.CurrentUser);
        return Convert.ToBase64String(bytes);
    }

    static string? Decrypt(string? value)
    {
        if (string.IsNullOrEmpty(value)) return null;
        try
        {
            var bytes = ProtectedData.Unprotect(Convert.FromBase64String(value), null, DataProtectionScope.CurrentUser);
            return System.Text.Encoding.UTF8.GetString(bytes);
        }
        catch { return null; }
    }

    sealed class StoreShape
    {
        public List<StoredConnection> Connections { get; set; } = [];
        public List<RecentSession> Recent { get; set; } = [];
    }

    sealed class StoredConnection
    {
        public string Id { get; set; } = "";
        public string Name { get; set; } = "";
        public string Host { get; set; } = "";
        public int Port { get; set; }
        public string Username { get; set; } = "";
        public string AuthType { get; set; } = "Password";
        public string? PrivateKeyPath { get; set; }
        public string? DefaultPath { get; set; }
        public long CreatedAt { get; set; }
        public long LastUsedAt { get; set; }
        public string? PasswordEnc { get; set; }
        public string? PassphraseEnc { get; set; }
    }
}
