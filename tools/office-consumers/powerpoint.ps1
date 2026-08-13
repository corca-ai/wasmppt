param(
    [Parameter(Mandatory = $true)][string]$InputPresentation,
    [Parameter(Mandatory = $true)][string]$OutputDirectory
)

$ErrorActionPreference = "Stop"
$inputPath = (Resolve-Path $InputPresentation).Path
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
$outputPath = (Resolve-Path $OutputDirectory).Path
$powerPoint = $null
$presentation = $null

try {
    $powerPoint = New-Object -ComObject PowerPoint.Application
    $powerPoint.AutomationSecurity = 3
    $powerPoint.DisplayAlerts = 1
    $presentation = $powerPoint.Presentations.Open($inputPath, $true, $false, $false)
    if ($presentation.Slides.Count -lt 1) {
        throw "PowerPoint opened a presentation with no slides"
    }
    $presentation.Export((Join-Path $outputPath "slides"), "PNG", 640, 360)
    $presentation.SaveAs((Join-Path $outputPath "ground-truth.pdf"), 32)
    $fontInventory = @($presentation.Fonts | ForEach-Object { $_.Name } | Sort-Object -Unique)
    $provenance = [ordered]@{
        schema = 1
        producer = "Microsoft PowerPoint"
        version = $powerPoint.Version
        platform = [System.Environment]::OSVersion.VersionString
        fixtureSha256 = (Get-FileHash -Algorithm SHA256 $inputPath).Hash.ToLowerInvariant()
        slideCount = $presentation.Slides.Count
        exportWidth = 640
        exportHeight = 360
        fonts = $fontInventory
        generatedAtUtc = [DateTime]::UtcNow.ToString("o")
    }
    $provenance | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 (Join-Path $outputPath "provenance.json")
    Write-Output "PowerPoint opened without repair and exported $($presentation.Slides.Count) slides"
}
finally {
    if ($null -ne $presentation) { $presentation.Close() }
    if ($null -ne $powerPoint) { $powerPoint.Quit() }
    if ($null -ne $presentation) { [void][Runtime.InteropServices.Marshal]::ReleaseComObject($presentation) }
    if ($null -ne $powerPoint) { [void][Runtime.InteropServices.Marshal]::ReleaseComObject($powerPoint) }
}
