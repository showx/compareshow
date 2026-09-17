namespace CompareShow.Core;

public sealed class CompareEngine(SftpService sftp)
{
    public async Task<FolderCompareResult> CompareFoldersAsync(
        LocationRef left,
        LocationRef right,
        CompareMode mode,
        IReadOnlyList<string> ignore,
        IProgress<(int Scanned, string Path)>? progress,
        CancellationToken ct)
    {
        var stats = new CompareStats();
        var scanned = 0;

        async Task<List<CompareNode>> Walk(LocationRef l, LocationRef r, string rel)
        {
            List<FileMeta> leftList = [];
            List<FileMeta> rightList = [];
            Exception? leftErr = null, rightErr = null;
            try { leftList = await ListSideAsync(l, ct); }
            catch (Exception ex) { leftErr = ex; }
            try { rightList = await ListSideAsync(r, ct); }
            catch (Exception ex) { rightErr = ex; }

            if (rel.Length == 0)
            {
                if (leftErr is not null) throw leftErr;
                if (rightErr is not null) throw rightErr;
            }

            var names = new SortedSet<string>(StringComparer.Create(TextUtil.Zh, true));
            var leftMap = new Dictionary<string, FileMeta>(StringComparer.Ordinal);
            var rightMap = new Dictionary<string, FileMeta>(StringComparer.Ordinal);
            foreach (var e in leftList)
            {
                if (MatchIgnore(e.Name, ignore)) continue;
                leftMap[e.Name] = e;
                names.Add(e.Name);
            }
            foreach (var e in rightList)
            {
                if (MatchIgnore(e.Name, ignore)) continue;
                rightMap[e.Name] = e;
                names.Add(e.Name);
            }

            var nodes = new List<CompareNode>();
            var sorted = names.OrderBy(n =>
            {
                var m = leftMap.GetValueOrDefault(n) ?? rightMap[n];
                return m.Type == EntryType.Dir ? 0 : 1;
            }).ThenBy(n => n, StringComparer.Create(TextUtil.Zh, true));

            foreach (var name in sorted)
            {
                ct.ThrowIfCancellationRequested();
                leftMap.TryGetValue(name, out var lm);
                rightMap.TryGetValue(name, out var rm);
                var type = (lm ?? rm)!.Type;
                var relPath = string.IsNullOrEmpty(rel) ? name : rel + "/" + name;
                scanned++;
                progress?.Report((scanned, relPath));

                var node = new CompareNode
                {
                    Name = name,
                    RelPath = relPath,
                    Type = type,
                    Left = lm,
                    Right = rm
                };

                if (type == EntryType.Dir)
                {
                    if (lm is not null && rm is not null)
                        node.Children = await Walk(Child(l, name), Child(r, name), relPath);
                    else if (lm is not null)
                        node.Children = await WalkOnly(Child(l, name), relPath, ItemStatus.LeftOnly, true);
                    else
                        node.Children = await WalkOnly(Child(r, name), relPath, ItemStatus.RightOnly, false);
                    node.Status = Rollup(node);
                }
                else
                {
                    node.Status = await FileStatusAsync(lm, rm, mode, l, r, ct);
                }
                Count(node, stats);
                nodes.Add(node);
            }
            return nodes;
        }

        async Task<List<CompareNode>> WalkOnly(LocationRef loc, string rel, ItemStatus status, bool leftSide)
        {
            List<FileMeta> list;
            try { list = await ListSideAsync(loc, ct); }
            catch { list = []; }
            var nodes = new List<CompareNode>();
            foreach (var entry in list)
            {
                if (MatchIgnore(entry.Name, ignore)) continue;
                ct.ThrowIfCancellationRequested();
                var relPath = string.IsNullOrEmpty(rel) ? entry.Name : rel + "/" + entry.Name;
                scanned++;
                progress?.Report((scanned, relPath));
                var node = new CompareNode
                {
                    Name = entry.Name,
                    RelPath = relPath,
                    Type = entry.Type,
                    Status = status,
                    Left = leftSide ? entry : null,
                    Right = leftSide ? null : entry
                };
                if (entry.Type == EntryType.Dir)
                    node.Children = await WalkOnly(Child(loc, entry.Name), relPath, status, leftSide);
                Count(node, stats);
                nodes.Add(node);
            }
            return nodes;
        }

        var tree = await Walk(left, right, "");
        return new FolderCompareResult { Tree = tree, Stats = stats };
    }

    public async Task<FileCompareResult> CompareFileAsync(LocationRef left, LocationRef right, CancellationToken ct)
    {
        var l = await ReadForCompareAsync(left, AppLimits.TextMaxBytes, ct);
        var r = await ReadForCompareAsync(right, AppLimits.TextMaxBytes, ct);
        if (l.TooLarge || r.TooLarge)
        {
            return new FileCompareResult
            {
                LeftMeta = l.Meta,
                RightMeta = r.Meta,
                TooLarge = true,
                LeftBinary = true,
                RightBinary = true
            };
        }

        var leftBinary = l.Buf is not null && TextUtil.IsProbablyBinary(l.Buf);
        var rightBinary = r.Buf is not null && TextUtil.IsProbablyBinary(r.Buf);
        var leftHash = l.Buf is null ? null : TextUtil.HashBytes(l.Buf);
        var rightHash = r.Buf is null ? null : TextUtil.HashBytes(r.Buf);
        var result = new FileCompareResult
        {
            LeftMeta = l.Meta,
            RightMeta = r.Meta,
            LeftBinary = leftBinary,
            RightBinary = rightBinary,
            LeftHash = leftHash,
            RightHash = rightHash,
            Identical = leftHash is not null && leftHash == rightHash
        };
        if (!leftBinary && l.Buf is not null)
        {
            var d = TextUtil.DecodeText(l.Buf);
            result.LeftText = d.Text;
            result.Encoding = d.Encoding;
        }
        if (!rightBinary && r.Buf is not null)
        {
            var d = TextUtil.DecodeText(r.Buf);
            result.RightText = d.Text;
        }
        return result;
    }

    async Task<(FileMeta? Meta, byte[]? Buf, bool TooLarge)> ReadForCompareAsync(LocationRef loc, long maxBytes, CancellationToken ct)
    {
        if (string.IsNullOrEmpty(loc.Path)) return (null, null, false);
        try
        {
            FileMeta meta;
            if (loc.Kind == LocationKind.Local) meta = LocalFs.Stat(loc.Path);
            else meta = await sftp.StatAsync(loc.ConnectionId!, loc.Path, ct);
            if (meta.Type == EntryType.Dir) return (meta, null, false);
            if (meta.Size > maxBytes) return (meta, null, true);
            var buf = loc.Kind == LocationKind.Local
                ? LocalFs.Read(loc.Path, maxBytes)
                : await sftp.ReadAsync(loc.ConnectionId!, loc.Path, maxBytes, ct);
            return (meta, buf, false);
        }
        catch (Exception ex) when (ex.Message == "FILE_TOO_LARGE")
        {
            return (null, null, true);
        }
        catch
        {
            return (null, null, false);
        }
    }

    async Task<List<FileMeta>> ListSideAsync(LocationRef loc, CancellationToken ct)
    {
        if (loc.Kind == LocationKind.Local) return await Task.Run(() => LocalFs.List(loc.Path), ct);
        if (string.IsNullOrEmpty(loc.ConnectionId)) throw new InvalidOperationException("缺少 SFTP 连接");
        return await sftp.ListAsync(loc.ConnectionId, loc.Path, ct);
    }

    static LocationRef Child(LocationRef loc, string name)
    {
        if (loc.Kind == LocationKind.Local)
            return new LocationRef { Kind = loc.Kind, Path = LocalFs.Join(loc.Path, name), ConnectionId = loc.ConnectionId };
        return new LocationRef { Kind = loc.Kind, Path = LocalFs.PosixJoin(loc.Path, name), ConnectionId = loc.ConnectionId };
    }

    async Task<ItemStatus> FileStatusAsync(FileMeta? left, FileMeta? right, CompareMode mode, LocationRef leftLoc, LocationRef rightLoc, CancellationToken ct)
    {
        if (left is not null && right is null) return ItemStatus.LeftOnly;
        if (left is null && right is not null) return ItemStatus.RightOnly;
        if (left is null || right is null) return ItemStatus.Unknown;
        if (left.Type != right.Type) return ItemStatus.Different;
        if (left.Type == EntryType.Dir) return ItemStatus.Same;
        if (left.Size != right.Size) return ItemStatus.Different;
        if (mode == CompareMode.Quick)
        {
            var lt = left.Mtime / 1000;
            var rt = right.Mtime / 1000;
            if (lt != rt && left.Size == right.Size) return ItemStatus.Different;
            return ItemStatus.Same;
        }
        try
        {
            var lh = leftLoc.Kind == LocationKind.Local
                ? await Task.Run(() => LocalFs.HashFile(left.Path), ct)
                : await sftp.HashAsync(leftLoc.ConnectionId!, left.Path, ct);
            var rh = rightLoc.Kind == LocationKind.Local
                ? await Task.Run(() => LocalFs.HashFile(right.Path), ct)
                : await sftp.HashAsync(rightLoc.ConnectionId!, right.Path, ct);
            return lh == rh ? ItemStatus.Same : ItemStatus.Different;
        }
        catch { return ItemStatus.Different; }
    }

    static bool MatchIgnore(string name, IReadOnlyList<string> patterns)
    {
        foreach (var p in patterns)
        {
            var t = p.Trim();
            if (t.Length == 0) continue;
            if (t.Contains('*'))
            {
                var re = "^" + System.Text.RegularExpressions.Regex.Escape(t).Replace("\\*", ".*") + "$";
                if (System.Text.RegularExpressions.Regex.IsMatch(name, re, System.Text.RegularExpressions.RegexOptions.IgnoreCase))
                    return true;
            }
            else if (string.Equals(name, t, StringComparison.OrdinalIgnoreCase)) return true;
        }
        return false;
    }

    static ItemStatus Rollup(CompareNode node)
    {
        if (node.Type == EntryType.File || node.Children is not { Count: > 0 }) return node.Status;
        var set = new HashSet<ItemStatus>();
        foreach (var c in node.Children)
            set.Add(c.Type == EntryType.Dir ? Rollup(c) : c.Status);
        if (set.Contains(ItemStatus.Different) || (set.Contains(ItemStatus.LeftOnly) && set.Contains(ItemStatus.RightOnly)))
            return node.Status = ItemStatus.Different;
        if (set.Count == 1) return node.Status = set.First();
        if (set.Contains(ItemStatus.LeftOnly)) return node.Status = ItemStatus.LeftOnly;
        if (set.Contains(ItemStatus.RightOnly)) return node.Status = ItemStatus.RightOnly;
        return node.Status = ItemStatus.Different;
    }

    static void Count(CompareNode node, CompareStats stats)
    {
        if (node.Type == EntryType.Dir) { stats.Dirs++; return; }
        stats.Files++;
        switch (node.Status)
        {
            case ItemStatus.Same: stats.Same++; break;
            case ItemStatus.Different: stats.Different++; break;
            case ItemStatus.LeftOnly: stats.LeftOnly++; break;
            case ItemStatus.RightOnly: stats.RightOnly++; break;
        }
    }
}
