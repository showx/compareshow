using System.Globalization;
using System.Security.Cryptography;
using System.Text;

namespace CompareShow.Core;

public static class TextUtil
{
    public static readonly CultureInfo Zh = CultureInfo.GetCultureInfo("zh-CN");

    public static string FormatSize(long? bytes)
    {
        if (bytes is null) return "";
        if (bytes == 0) return "0 B";
        string[] units = ["B", "KB", "MB", "GB", "TB"];
        double n = bytes.Value;
        var i = 0;
        while (n >= 1024 && i < units.Length - 1)
        {
            n /= 1024;
            i++;
        }
        return (n < 10 && i > 0 ? n.ToString("0.0") : Math.Round(n).ToString(CultureInfo.InvariantCulture)) + " " + units[i];
    }

    public static string FormatTime(long ms)
    {
        if (ms <= 0) return "";
        var d = DateTimeOffset.FromUnixTimeMilliseconds(ms).ToLocalTime().DateTime;
        return d.ToString("yyyy-MM-dd HH:mm");
    }

    public static long ToUnixMs(DateTime dt)
    {
        var utc = dt.Kind == DateTimeKind.Unspecified
            ? DateTime.SpecifyKind(dt, DateTimeKind.Local).ToUniversalTime()
            : dt.ToUniversalTime();
        return new DateTimeOffset(utc).ToUnixTimeMilliseconds();
    }

    public static string HashStream(Stream stream)
    {
        using var md5 = MD5.Create();
        return Convert.ToHexString(md5.ComputeHash(stream)).ToLowerInvariant();
    }

    public static string HashBytes(byte[] data)
    {
        return Convert.ToHexString(MD5.HashData(data)).ToLowerInvariant();
    }

    public static bool IsProbablyBinary(ReadOnlySpan<byte> buf)
    {
        var sample = buf.Length > 8192 ? buf[..8192] : buf;
        if (sample.IndexOf((byte)0) >= 0) return true;
        var weird = 0;
        foreach (var b in sample)
        {
            if (b < 7 || (b > 14 && b < 32 && b != 9 && b != 10 && b != 13)) weird++;
        }
        return sample.Length > 0 && weird / (double)sample.Length > 0.3;
    }

    public static (string Text, string Encoding) DecodeText(byte[] buf)
    {
        if (buf.Length >= 2 && buf[0] == 0xFF && buf[1] == 0xFE)
            return (Encoding.Unicode.GetString(buf, 2, buf.Length - 2), "utf-16le");
        if (buf.Length >= 3 && buf[0] == 0xEF && buf[1] == 0xBB && buf[2] == 0xBF)
            return (Encoding.UTF8.GetString(buf, 3, buf.Length - 3), "utf-8");
        return (Encoding.UTF8.GetString(buf), "utf-8");
    }

    public static byte[] EncodeText(string text, string? encoding)
    {
        if (string.Equals(encoding, "utf-16le", StringComparison.OrdinalIgnoreCase))
        {
            var bom = Encoding.Unicode.GetPreamble();
            var body = Encoding.Unicode.GetBytes(text);
            var all = new byte[bom.Length + body.Length];
            Buffer.BlockCopy(bom, 0, all, 0, bom.Length);
            Buffer.BlockCopy(body, 0, all, bom.Length, body.Length);
            return all;
        }
        return Encoding.UTF8.GetBytes(text);
    }
}
