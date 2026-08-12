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
    Write-Output "PowerPoint opened without repair and exported $($presentation.Slides.Count) slides"
}
finally {
    if ($null -ne $presentation) { $presentation.Close() }
    if ($null -ne $powerPoint) { $powerPoint.Quit() }
    if ($null -ne $presentation) { [void][Runtime.InteropServices.Marshal]::ReleaseComObject($presentation) }
    if ($null -ne $powerPoint) { [void][Runtime.InteropServices.Marshal]::ReleaseComObject($powerPoint) }
}
