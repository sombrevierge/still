using System.Diagnostics;
using System.Security.Principal;
using System.Text.Json;
using System.Text.Json.Serialization;
using LibreHardwareMonitor.Hardware;

namespace Still.HardwareSensors;

internal sealed record SensorReading(
    string Hardware,
    string HardwareType,
    string Name,
    string SensorType,
    float Value);

internal sealed record SensorSnapshot
{
    public long CapturedAtMs { get; init; } = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
    public float? CpuTempC { get; init; }
    public float? GpuTempC { get; init; }
    public float? StorageTempC { get; init; }
    public string? StorageName { get; init; }
    public string? StorageSensorName { get; init; }
    public float? GpuUsagePercent { get; init; }
    public ulong? GpuVramUsedBytes { get; init; }
    public float? CpuPackagePowerW { get; init; }
    public float? GpuPowerW { get; init; }
    public float? FanRpm { get; init; }
    public float? CpuClockMhz { get; init; }
    public float? GpuClockMhz { get; init; }
    public bool Elevated { get; init; }
    public string Source { get; init; } = "LibreHardwareMonitor 0.9.6";
    public string Status { get; init; } = "No supported sensors reported";
    public IReadOnlyList<SensorReading> Sensors { get; init; } = [];
}

internal sealed class UpdateVisitor : IVisitor
{
    public void VisitComputer(IComputer computer) => computer.Traverse(this);
    public void VisitHardware(IHardware hardware)
    {
        hardware.Update();
        foreach (var subHardware in hardware.SubHardware)
        {
            subHardware.Accept(this);
        }
    }
    public void VisitSensor(ISensor sensor) { }
    public void VisitParameter(IParameter parameter) { }
}

internal static class Program
{
    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull,
        WriteIndented = false,
    };

    public static async Task<int> Main(string[] args)
    {
        var watch = Array.IndexOf(args, "--watch") >= 0;
        var output = Argument(args, "--output");
        var parentText = Argument(args, "--parent");
        var parentPid = int.TryParse(parentText, out var parsedPid) ? parsedPid : 0;

        var computer = new Computer
        {
            IsCpuEnabled = true,
            IsGpuEnabled = true,
            IsMemoryEnabled = true,
            IsMotherboardEnabled = true,
            IsControllerEnabled = true,
            IsStorageEnabled = true,
            IsPowerMonitorEnabled = true,
        };

        try
        {
            computer.Open();
            if (!watch)
            {
                computer.Accept(new UpdateVisitor());
                Console.WriteLine(JsonSerializer.Serialize(Capture(computer), JsonOptions));
                return 0;
            }

            if (string.IsNullOrWhiteSpace(output) || parentPid <= 0)
            {
                return 2;
            }
            Directory.CreateDirectory(Path.GetDirectoryName(output)!);
            while (ParentIsAlive(parentPid))
            {
                computer.Accept(new UpdateVisitor());
                await WriteAtomically(output, JsonSerializer.Serialize(Capture(computer), JsonOptions));
                await Task.Delay(TimeSpan.FromSeconds(5));
            }
            return 0;
        }
        catch (Exception error)
        {
            var failed = new SensorSnapshot
            {
                Elevated = IsElevated(),
                Status = $"Sensor provider failed: {error.GetType().Name}",
            };
            var json = JsonSerializer.Serialize(failed, JsonOptions);
            if (!string.IsNullOrWhiteSpace(output))
            {
                await WriteAtomically(output, json);
            }
            else
            {
                Console.WriteLine(json);
            }
            return 1;
        }
        finally
        {
            computer.Close();
        }
    }

    private static SensorSnapshot Capture(Computer computer)
    {
        var readings = computer.Hardware
            .SelectMany(ReadHardware)
            .Where(reading => float.IsFinite(reading.Value))
            .ToArray();
        var cpu = readings.Where(reading => reading.HardwareType.Equals("Cpu", StringComparison.OrdinalIgnoreCase)).ToArray();
        var gpu = readings.Where(reading => reading.HardwareType.StartsWith("Gpu", StringComparison.OrdinalIgnoreCase)).ToArray();
        var storage = readings.Where(reading => reading.HardwareType.Equals("Storage", StringComparison.OrdinalIgnoreCase)).ToArray();

        var cpuTemp = Pick(cpu, "Temperature", ["Package", "Tctl", "Tdie", "Core Max", "Core Average"]);
        var gpuTemp = Pick(gpu, "Temperature", ["GPU Core", "Core", "Hot Spot"]);
        var storageTemp = CurrentStorageTemperature(storage);
        // On AMD APUs the generic GPU Core counter can stay at zero while the
        // D3D engine counter reports the activity visible in Task Manager.
        var gpuUsage = Pick(gpu, "Load", ["D3D 3D", "GPU Core", "Core"]);
        var vramMb = Pick(gpu, "SmallData", ["GPU Memory Used", "D3D Dedicated Memory Used", "Memory Used"]);
        var cpuPower = Pick(cpu, "Power", ["CPU Package", "Package", "Cores"]);
        var gpuPower = Pick(gpu, "Power", ["GPU Package", "GPU Core", "Total"]);
        var fan = Max(readings, "Fan");
        var cpuClock = Average(cpu.Where(reading => reading.SensorType == "Clock" && reading.Name.Contains("Core", StringComparison.OrdinalIgnoreCase)));
        var gpuClock = Pick(gpu, "Clock", ["GPU Core", "Core"]);
        var elevated = IsElevated();
        var found = readings.Count(reading => reading.SensorType is "Temperature" or "Fan" or "Power" or "Clock" or "Load");

        return new SensorSnapshot
        {
            CpuTempC = ValidTemperature(cpuTemp),
            GpuTempC = ValidTemperature(gpuTemp),
            StorageTempC = storageTemp?.Value,
            StorageName = storageTemp?.Hardware,
            StorageSensorName = storageTemp?.Name,
            GpuUsagePercent = ValidPercent(gpuUsage),
            GpuVramUsedBytes = vramMb is > 0 ? (ulong)(vramMb.Value * 1_048_576f) : null,
            CpuPackagePowerW = Positive(cpuPower),
            GpuPowerW = Positive(gpuPower),
            FanRpm = Positive(fan),
            CpuClockMhz = Positive(cpuClock),
            GpuClockMhz = Positive(gpuClock),
            Elevated = elevated,
            Status = found > 0
                ? $"{found} live hardware readings{(elevated ? " with administrator access" : " with standard access")}"
                : $"No supported sensors reported{(elevated ? " with administrator access" : "; administrator access may reveal more")}",
            Sensors = readings,
        };
    }

    private static IEnumerable<SensorReading> ReadHardware(IHardware hardware)
    {
        foreach (var sensor in hardware.Sensors)
        {
            if (sensor.Value is { } value)
            {
                yield return new SensorReading(hardware.Name, hardware.HardwareType.ToString(), sensor.Name, sensor.SensorType.ToString(), value);
            }
        }
        foreach (var subHardware in hardware.SubHardware)
        {
            foreach (var reading in ReadHardware(subHardware))
            {
                yield return reading;
            }
        }
    }

    private static float? Pick(IEnumerable<SensorReading> readings, string sensorType, string[] priorities)
    {
        var candidates = readings.Where(reading => reading.SensorType == sensorType).ToArray();
        foreach (var priority in priorities)
        {
            var match = candidates.FirstOrDefault(reading => reading.Name.Contains(priority, StringComparison.OrdinalIgnoreCase));
            if (match is not null) return match.Value;
        }
        return candidates.FirstOrDefault()?.Value;
    }

    private static float? Max(IEnumerable<SensorReading> readings, string sensorType)
    {
        var values = readings.Where(reading => reading.SensorType == sensorType).Select(reading => reading.Value).ToArray();
        return values.Length > 0 ? values.Max() : null;
    }

    private static SensorReading? CurrentStorageTemperature(IEnumerable<SensorReading> readings)
    {
        var temperatures = readings
            .Where(reading =>
                reading.SensorType == "Temperature"
                && ValidTemperature(reading.Value) is not null
                && !reading.Name.Contains("Warning", StringComparison.OrdinalIgnoreCase)
                && !reading.Name.Contains("Critical", StringComparison.OrdinalIgnoreCase)
                && !reading.Name.Contains("Threshold", StringComparison.OrdinalIgnoreCase)
                && !reading.Name.Contains("Limit", StringComparison.OrdinalIgnoreCase))
            .ToArray();
        return temperatures.FirstOrDefault(reading =>
                   reading.Name.Contains("Composite", StringComparison.OrdinalIgnoreCase))
               ?? temperatures.FirstOrDefault(reading =>
                   reading.Name.Equals("Temperature", StringComparison.OrdinalIgnoreCase))
               ?? temperatures.OrderByDescending(reading => reading.Value).FirstOrDefault();
    }

    private static float? Average(IEnumerable<SensorReading> readings)
    {
        var values = readings.Select(reading => reading.Value).Where(value => value > 0).ToArray();
        return values.Length > 0 ? values.Average() : null;
    }

    private static float? ValidTemperature(float? value) => value is > 1 and < 150 ? value : null;
    private static float? ValidPercent(float? value) => value is >= 0 and <= 100 ? value : null;
    private static float? Positive(float? value) => value is > 0 ? value : null;

    private static bool IsElevated()
    {
        using var identity = WindowsIdentity.GetCurrent();
        return new WindowsPrincipal(identity).IsInRole(WindowsBuiltInRole.Administrator);
    }

    private static bool ParentIsAlive(int pid)
    {
        try { return !Process.GetProcessById(pid).HasExited; }
        catch { return false; }
    }

    private static string? Argument(string[] args, string name)
    {
        var index = Array.IndexOf(args, name);
        return index >= 0 && index + 1 < args.Length ? args[index + 1] : null;
    }

    private static async Task WriteAtomically(string path, string contents)
    {
        var temporary = path + ".tmp";
        await File.WriteAllTextAsync(temporary, contents);
        File.Move(temporary, path, true);
    }
}
