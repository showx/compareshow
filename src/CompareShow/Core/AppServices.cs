namespace CompareShow.Core;

public sealed class AppServices : IDisposable
{
    public SessionStore Store { get; } = new();
    public SftpService Sftp { get; } = new();
    public CompareEngine Compare { get; }
    public TransferService Transfer { get; }

    public AppServices()
    {
        Compare = new CompareEngine(Sftp);
        Transfer = new TransferService(Sftp);
    }

    public async Task EnsureConnectedAsync(LocationRef loc, CancellationToken ct = default)
    {
        if (loc.Kind != LocationKind.Sftp || string.IsNullOrEmpty(loc.ConnectionId)) return;
        if (Sftp.HasConnection(loc.ConnectionId)) return;
        var cfg = Store.GetConnection(loc.ConnectionId)
            ?? throw new InvalidOperationException("SFTP 连接不存在，请先保存并测试连接");
        await Sftp.ConnectAsync(cfg, ct);
        Store.Touch(cfg.Id);
    }

    public async Task<OpenSftpResult> OpenUrlAsync(string url, string? password, bool remember, CancellationToken ct = default)
    {
        var parsed = SftpUrl.Parse(url);
        if (parsed is null)
            return new OpenSftpResult { Error = "无法解析地址。请使用 ssh://root@主机 或 sftp://root@主机/路径" };

        var resolved = SshAuth.Resolve(parsed.Host, parsed.Username, parsed.Port);
        var username = parsed.Username ?? resolved.Username;
        var host = parsed.Host;
        var port = parsed.Port ?? resolved.Port;
        var path = string.IsNullOrEmpty(parsed.Path) ? "/" : parsed.Path;

        var cfg = Store.FindByEndpoint(username, host, port)
                  ?? Store.FindByEndpoint(username, parsed.Host, port);
        var pass = password ?? parsed.Password ?? cfg?.Password;
        var now = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
        cfg = Store.Upsert(new SftpConnectionConfig
        {
            Id = cfg?.Id ?? Guid.NewGuid().ToString("N"),
            Name = cfg?.Name ?? $"{username}@{parsed.Host}",
            Host = parsed.Host,
            Port = port,
            Username = username,
            AuthType = !string.IsNullOrEmpty(pass) ? AuthType.Password : (string.IsNullOrEmpty(cfg?.PrivateKeyPath) ? AuthType.Password : AuthType.PrivateKey),
            Password = remember ? pass : cfg?.Password,
            PrivateKeyPath = cfg?.PrivateKeyPath,
            Passphrase = password is not null && parsed.Password is null ? (cfg?.Passphrase ?? password) : cfg?.Passphrase,
            DefaultPath = path,
            CreatedAt = cfg?.CreatedAt is > 0 ? cfg.CreatedAt : now,
            LastUsedAt = now
        });

        try
        {
            var live = cfg.Clone();
            live.Username = username;
            live.Host = parsed.Host;
            live.Port = port;
            live.Password = pass;
            if (password is not null && parsed.Password is null)
                live.Passphrase = cfg.Passphrase ?? password;
            await Sftp.ConnectAsync(live, ct);
        }
        catch (SshConnectException ex) when (ex.Code == "HOST")
        {
            return new OpenSftpResult { Error = ex.Message };
        }
        catch (Exception ex) when (ex is SshConnectException { Code: "AUTH" } || SshAuth.IsAuthFailure(ex))
        {
            var keys = ex is SshConnectException s ? s.KeysTried.ToArray() : [];
            return new OpenSftpResult
            {
                NeedPassword = true,
                Error = ex.Message,
                Username = username,
                Host = parsed.Host,
                Port = port,
                KeysTried = keys
            };
        }
        catch (Exception ex)
        {
            return new OpenSftpResult { Error = ex.Message };
        }

        Store.Touch(cfg.Id);
        return new OpenSftpResult
        {
            Ok = true,
            ConnectionId = cfg.Id,
            Path = path,
            DisplayUrl = SftpUrl.Format(username, parsed.Host, port, path, parsed.Scheme == "ssh" ? "ssh" : "sftp"),
            Connection = Strip(cfg)
        };
    }

    public static SftpConnectionConfig Strip(SftpConnectionConfig c)
    {
        var x = c.Clone();
        if (!string.IsNullOrEmpty(x.Password)) x.Password = "********";
        if (!string.IsNullOrEmpty(x.Passphrase)) x.Passphrase = "********";
        return x;
    }

    public void Dispose() => Sftp.Dispose();
}
