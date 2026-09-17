namespace CompareShow.Core;

public sealed class TransferService(SftpService sftp)
{
    public async Task CopyAsync(LocationRef source, LocationRef targetDir, bool isDir, CancellationToken ct = default)
    {
        var name = source.Kind == LocationKind.Local
            ? Path.GetFileName(source.Path.TrimEnd('\\', '/'))
            : source.Path.Split('/', StringSplitOptions.RemoveEmptyEntries).LastOrDefault() ?? "item";
        var destPath = targetDir.Kind == LocationKind.Local
            ? LocalFs.Join(targetDir.Path, name)
            : LocalFs.PosixJoin(targetDir.Path, name);
        var dest = new LocationRef { Kind = targetDir.Kind, Path = destPath, ConnectionId = targetDir.ConnectionId };
        if (isDir) await CopyDirAsync(source, dest, ct);
        else await CopyFileAsync(source, dest, ct);
    }

    public async Task WriteTextAsync(LocationRef loc, string text, string? encoding, CancellationToken ct = default)
    {
        if (string.IsNullOrEmpty(loc.Path)) throw new InvalidOperationException("没有可保存的路径");
        var bytes = TextUtil.EncodeText(text, encoding);
        if (loc.Kind == LocationKind.Local)
        {
            await Task.Run(() => LocalFs.Write(loc.Path, bytes), ct);
            return;
        }
        await sftp.WriteAsync(loc.ConnectionId!, loc.Path, bytes, ct);
    }

    public async Task RemoveAsync(LocationRef loc, bool isDir, CancellationToken ct = default)
    {
        if (loc.Kind == LocationKind.Local)
        {
            await Task.Run(() => LocalFs.Remove(loc.Path), ct);
            return;
        }
        await sftp.RemoveAsync(loc.ConnectionId!, loc.Path, isDir, ct);
    }

    async Task CopyFileAsync(LocationRef source, LocationRef dest, CancellationToken ct)
    {
        if (source.Kind == LocationKind.Local && dest.Kind == LocationKind.Local)
        {
            await Task.Run(() => LocalFs.CopyFile(source.Path, dest.Path), ct);
            return;
        }
        if (source.Kind == LocationKind.Local && dest.Kind == LocationKind.Sftp)
        {
            await sftp.UploadFromAsync(dest.ConnectionId!, source.Path, dest.Path, ct);
            return;
        }
        if (source.Kind == LocationKind.Sftp && dest.Kind == LocationKind.Local)
        {
            await sftp.DownloadToAsync(source.ConnectionId!, source.Path, dest.Path, ct);
            return;
        }
        await sftp.CopyRemoteAsync(source.ConnectionId!, source.Path, dest.ConnectionId!, dest.Path, ct);
    }

    async Task CopyDirAsync(LocationRef source, LocationRef dest, CancellationToken ct)
    {
        if (dest.Kind == LocationKind.Local) await Task.Run(() => LocalFs.Mkdir(dest.Path), ct);
        else await sftp.MkdirAsync(dest.ConnectionId!, dest.Path, ct);

        var children = source.Kind == LocationKind.Local
            ? await Task.Run(() => LocalFs.List(source.Path), ct)
            : await sftp.ListAsync(source.ConnectionId!, source.Path, ct);

        foreach (var child in children)
        {
            ct.ThrowIfCancellationRequested();
            var nextSrc = new LocationRef
            {
                Kind = source.Kind,
                ConnectionId = source.ConnectionId,
                Path = source.Kind == LocationKind.Local ? LocalFs.Join(source.Path, child.Name) : LocalFs.PosixJoin(source.Path, child.Name)
            };
            var nextDest = new LocationRef
            {
                Kind = dest.Kind,
                ConnectionId = dest.ConnectionId,
                Path = dest.Kind == LocationKind.Local ? LocalFs.Join(dest.Path, child.Name) : LocalFs.PosixJoin(dest.Path, child.Name)
            };
            if (child.Type == EntryType.Dir) await CopyDirAsync(nextSrc, nextDest, ct);
            else await CopyFileAsync(nextSrc, nextDest, ct);
        }
    }
}
