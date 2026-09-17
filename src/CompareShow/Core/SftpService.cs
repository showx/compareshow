using System.Collections.Concurrent;
using Renci.SshNet;
using Renci.SshNet.Common;

namespace CompareShow.Core;

public sealed class SftpService : IDisposable
{
    readonly ConcurrentDictionary<string, Live> _pool = new();

    sealed class Live : IDisposable
    {
        public required SftpClient Client { get; init; }
        public required SftpConnectionConfig Config { get; init; }
        public readonly SemaphoreSlim Gate = new(1, 1);

        public void Dispose()
        {
            try { if (Client.IsConnected) Client.Disconnect(); } catch { /* ignore */ }
            try { Client.Dispose(); } catch { /* ignore */ }
            Gate.Dispose();
        }
    }

    public bool HasConnection(string id) => _pool.TryGetValue(id, out var live) && live.Client.IsConnected;

    public async Task ConnectAsync(SftpConnectionConfig config, CancellationToken ct = default)
    {
        await DisconnectAsync(config.Id);
        var resolved = SshAuth.Resolve(config.Host, string.IsNullOrEmpty(config.Username) ? null : config.Username, config.Port);
        var username = string.IsNullOrEmpty(config.Username) ? resolved.Username : config.Username;
        var host = resolved.Host;
        var port = config.Port > 0 ? config.Port : resolved.Port;
        var keyFiles = new List<string>();
        if (!string.IsNullOrEmpty(config.PrivateKeyPath) && File.Exists(config.PrivateKeyPath))
            keyFiles.Add(config.PrivateKeyPath);
        foreach (var f in resolved.IdentityFiles)
            if (!keyFiles.Contains(f, StringComparer.OrdinalIgnoreCase)) keyFiles.Add(f);

        Exception? lastAuth = null;
        SftpClient? client = null;
        var hostChecked = false;

        foreach (var file in keyFiles)
        {
            ct.ThrowIfCancellationRequested();
            try
            {
                client = await TryOpenAsync(host, port, username, file, config.Passphrase, null, ct);
                break;
            }
            catch (Exception ex) when (SshAuth.IsHostUnreachable(ex))
            {
                throw new SshConnectException($"连不上 {host}:{port}。请确认服务器 SSH 已开、安全组放行 22 端口、本机网络可达。", "HOST", keyFiles);
            }
            catch (Exception ex) when (SshAuth.IsAuthFailure(ex))
            {
                lastAuth = ex;
                hostChecked = true;
            }
        }

        if (client is null && !string.IsNullOrEmpty(config.Password))
        {
            try
            {
                client = await TryOpenAsync(host, port, username, null, null, config.Password, ct);
            }
            catch (Exception ex) when (SshAuth.IsHostUnreachable(ex))
            {
                throw new SshConnectException($"连不上 {host}:{port}。请确认服务器 SSH 已开、安全组放行 22 端口、本机网络可达。", "HOST", keyFiles);
            }
            catch (Exception ex) when (SshAuth.IsAuthFailure(ex))
            {
                lastAuth = ex;
                hostChecked = true;
            }
            catch (Exception ex)
            {
                if (!hostChecked) throw;
                lastAuth = ex;
            }
        }

        if (client is null)
        {
            throw new SshConnectException(
                keyFiles.Count > 0
                    ? $"本机密钥未被服务器接受（已试：{string.Join("、", keyFiles.Select(Path.GetFileName))}）。请输入密码。"
                    : "SSH 认证失败，请输入密码。本机没有可用的 SSH 密钥。",
                "AUTH",
                keyFiles);
        }

        _pool[config.Id] = new Live { Client = client, Config = config };
        _ = lastAuth;
    }

    public Task DisconnectAsync(string id)
    {
        if (_pool.TryRemove(id, out var live)) live.Dispose();
        return Task.CompletedTask;
    }

    public void DisconnectAll()
    {
        foreach (var id in _pool.Keys.ToArray())
            if (_pool.TryRemove(id, out var live)) live.Dispose();
    }

    public async Task<List<FileMeta>> ListAsync(string id, string dir, CancellationToken ct = default)
    {
        var live = Require(id);
        await live.Gate.WaitAsync(ct);
        try
        {
            var target = string.IsNullOrEmpty(dir) ? "/" : dir;
            var list = await Task.Run(() => live.Client.ListDirectory(target).ToList(), ct);
            var items = list
                .Where(e => e.Name is not "." and not "..")
                .Select(e => ToMeta(target, e.Name, e.IsDirectory, e.Length, e.LastWriteTime, null))
                .ToList();
            items.Sort((a, b) =>
            {
                if (a.Type != b.Type) return a.Type == EntryType.Dir ? -1 : 1;
                return string.Compare(a.Name, b.Name, TextUtil.Zh, System.Globalization.CompareOptions.IgnoreCase);
            });
            return items;
        }
        finally { live.Gate.Release(); }
    }

    public async Task<FileMeta> StatAsync(string id, string path, CancellationToken ct = default)
    {
        var live = Require(id);
        await live.Gate.WaitAsync(ct);
        try
        {
            var attrs = await Task.Run(() => live.Client.GetAttributes(path), ct);
            var name = path.Split('/', StringSplitOptions.RemoveEmptyEntries).LastOrDefault() ?? path;
            return ToMeta(ParentRemote(path), name, attrs.IsDirectory, attrs.Size, attrs.LastWriteTime, null);
        }
        finally { live.Gate.Release(); }
    }

    public async Task<byte[]> ReadAsync(string id, string path, long? maxBytes = null, CancellationToken ct = default)
    {
        var live = Require(id);
        await live.Gate.WaitAsync(ct);
        try
        {
            return await Task.Run(() =>
            {
                using var ms = new MemoryStream();
                using var stream = live.Client.OpenRead(path);
                var buf = new byte[64 * 1024];
                long total = 0;
                int n;
                while ((n = stream.Read(buf, 0, buf.Length)) > 0)
                {
                    total += n;
                    if (maxBytes is > 0 && total > maxBytes) throw new InvalidOperationException("FILE_TOO_LARGE");
                    ms.Write(buf, 0, n);
                }
                return ms.ToArray();
            }, ct);
        }
        finally { live.Gate.Release(); }
    }

    public async Task WriteAsync(string id, string path, byte[] data, CancellationToken ct = default)
    {
        var live = Require(id);
        await live.Gate.WaitAsync(ct);
        try
        {
            await MkdirLockedAsync(live, ParentRemote(path));
            await Task.Run(() =>
            {
                using var ms = new MemoryStream(data);
                live.Client.UploadFile(ms, path, true);
            }, ct);
        }
        finally { live.Gate.Release(); }
    }

    public async Task MkdirAsync(string id, string path, CancellationToken ct = default)
    {
        var live = Require(id);
        await live.Gate.WaitAsync(ct);
        try { await MkdirLockedAsync(live, path); }
        finally { live.Gate.Release(); }
    }

    public async Task RemoveAsync(string id, string path, bool isDir, CancellationToken ct = default)
    {
        if (!isDir)
        {
            var live = Require(id);
            await live.Gate.WaitAsync(ct);
            try { await Task.Run(() => live.Client.DeleteFile(path), ct); }
            finally { live.Gate.Release(); }
            return;
        }
        var children = await ListAsync(id, path, ct);
        foreach (var child in children)
            await RemoveAsync(id, child.Path, child.Type == EntryType.Dir, ct);
        var live2 = Require(id);
        await live2.Gate.WaitAsync(ct);
        try { await Task.Run(() => live2.Client.DeleteDirectory(path), ct); }
        finally { live2.Gate.Release(); }
    }

    public async Task DownloadToAsync(string id, string remote, string local, CancellationToken ct = default)
    {
        var live = Require(id);
        await live.Gate.WaitAsync(ct);
        try
        {
            Directory.CreateDirectory(Path.GetDirectoryName(local)!);
            await Task.Run(() =>
            {
                using var fs = File.Create(local);
                live.Client.DownloadFile(remote, fs);
            }, ct);
        }
        finally { live.Gate.Release(); }
    }

    public async Task UploadFromAsync(string id, string local, string remote, CancellationToken ct = default)
    {
        var live = Require(id);
        await live.Gate.WaitAsync(ct);
        try
        {
            await MkdirLockedAsync(live, ParentRemote(remote));
            await Task.Run(() =>
            {
                using var fs = File.OpenRead(local);
                live.Client.UploadFile(fs, remote, true);
            }, ct);
        }
        finally { live.Gate.Release(); }
    }

    public async Task CopyRemoteAsync(string srcId, string srcPath, string destId, string destPath, CancellationToken ct = default)
    {
        var data = await ReadAsync(srcId, srcPath, null, ct);
        await WriteAsync(destId, destPath, data, ct);
    }

    public async Task<string> HashAsync(string id, string path, CancellationToken ct = default)
    {
        var live = Require(id);
        await live.Gate.WaitAsync(ct);
        try
        {
            return await Task.Run(() =>
            {
                using var stream = live.Client.OpenRead(path);
                return TextUtil.HashStream(stream);
            }, ct);
        }
        finally { live.Gate.Release(); }
    }

    public void Dispose() => DisconnectAll();

    Live Require(string id)
    {
        if (!_pool.TryGetValue(id, out var live) || !live.Client.IsConnected)
            throw new InvalidOperationException("SFTP 未连接，请先测试并连接服务器");
        return live;
    }

    static async Task MkdirLockedAsync(Live live, string path)
    {
        if (string.IsNullOrEmpty(path) || path == "/") return;
        var parts = path.Split('/', StringSplitOptions.RemoveEmptyEntries);
        var cur = "";
        foreach (var part in parts)
        {
            cur += "/" + part;
            var target = cur;
            await Task.Run(() =>
            {
                try { live.Client.CreateDirectory(target); }
                catch (SshException)
                {
                    try
                    {
                        var attrs = live.Client.GetAttributes(target);
                        if (!attrs.IsDirectory) throw;
                    }
                    catch { throw; }
                }
            });
        }
    }

    static async Task<SftpClient> TryOpenAsync(string host, int port, string username, string? keyFile, string? passphrase, string? password, CancellationToken ct)
    {
        return await Task.Run(() =>
        {
            var methods = new List<AuthenticationMethod>();
            if (!string.IsNullOrEmpty(keyFile) && File.Exists(keyFile))
            {
                PrivateKeyFile pk;
                try { pk = string.IsNullOrEmpty(passphrase) ? new PrivateKeyFile(keyFile) : new PrivateKeyFile(keyFile, passphrase); }
                catch (Exception ex) { throw new SshConnectException("无法读取私钥：" + ex.Message, "AUTH", [keyFile]); }
                methods.Add(new PrivateKeyAuthenticationMethod(username, pk));
            }
            if (!string.IsNullOrEmpty(password))
            {
                methods.Add(new PasswordAuthenticationMethod(username, password));
                var kbd = new KeyboardInteractiveAuthenticationMethod(username);
                kbd.AuthenticationPrompt += (_, e) =>
                {
                    foreach (var p in e.Prompts) p.Response = password;
                };
                methods.Add(kbd);
            }
            if (methods.Count == 0)
                throw new SshConnectException("没有可用的认证方式", "AUTH");

            var info = new ConnectionInfo(host, port, username, methods.ToArray())
            {
                Timeout = TimeSpan.FromSeconds(12)
            };
            var client = new SftpClient(info) { KeepAliveInterval = TimeSpan.FromSeconds(15), OperationTimeout = TimeSpan.FromSeconds(30) };
            try
            {
                ct.ThrowIfCancellationRequested();
                client.Connect();
                return client;
            }
            catch
            {
                client.Dispose();
                throw;
            }
        }, ct);
    }

    static FileMeta ToMeta(string dir, string filename, bool isDir, long size, DateTime mtime, uint? mode)
    {
        var path = dir == "/" ? "/" + filename : dir.TrimEnd('/') + "/" + filename;
        return new FileMeta
        {
            Name = filename,
            Path = path,
            Type = isDir ? EntryType.Dir : EntryType.File,
            Size = isDir ? 0 : size,
            Mtime = TextUtil.ToUnixMs(mtime),
            Mode = mode is null ? null : (int)mode.Value
        };
    }

    static string ParentRemote(string path)
    {
        var i = path.LastIndexOf('/');
        if (i <= 0) return "/";
        return path[..i];
    }
}
