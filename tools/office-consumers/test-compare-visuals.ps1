$ErrorActionPreference = "Stop"
$root = Join-Path ([IO.Path]::GetTempPath()) "wasmppt-visual-gate-$([Guid]::NewGuid())"
$expected = Join-Path $root "expected"
$actual = Join-Path $root "actual"
$manifestPath = Join-Path $root "manifest.json"
$reportPath = Join-Path $root "report.json"
$provenancePath = Join-Path $root "provenance.json"

try {
    New-Item -ItemType Directory -Force $expected, $actual | Out-Null
    & magick -size 2x2 xc:white (Join-Path $expected "Slide1.PNG")
    if ($LASTEXITCODE -ne 0) { throw "ImageMagick could not create the reference image" }
    & magick -size 2x2 xc:white -fill red -draw "point 0,0" (Join-Path $actual "slide-1-actual.png")
    if ($LASTEXITCODE -ne 0) { throw "ImageMagick could not create the one-pixel regression" }

    [ordered]@{
        schema = 1
        fixture = "self-test"
        fixtureSha256 = "self-test"
        slideCount = 1
        exportWidth = 2
        exportHeight = 2
        redistribution = "generated test image"
        owner = "wasmppt maintainers"
        regions = @([ordered]@{
            slideIndex = 0
            name = "one-pixel"
            x = 0
            y = 0
            width = 2
            height = 2
            tolerance = 0
        })
    } | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 $manifestPath
    [ordered]@{
        fixtureSha256 = "self-test"
        exportWidth = 2
        exportHeight = 2
        slideCount = 1
        fonts = @("self-test-font")
    } | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 $provenancePath

    $failedClosed = $false
    try {
        & "$PSScriptRoot/compare-visuals.ps1" $expected $actual $reportPath `
            -MaximumDifferentPixelRatio 0 -ManifestPath $manifestPath
    }
    catch {
        if ($_.Exception.Message -notlike "*exceeded the declared tolerance*") { throw }
        $failedClosed = $true
    }
    if (!$failedClosed) { throw "visual comparison accepted a deliberate one-pixel regression" }
    if (!(Test-Path $reportPath)) { throw "visual comparison did not emit an actionable report" }
    $report = Get-Content -Raw $reportPath | ConvertFrom-Json
    if ($report.slides[0].passed -or $report.regions[0].passed) {
        throw "visual comparison report did not identify the deliberate regression"
    }
    if (!(Test-Path (Join-Path $actual "slide-1-difference.png"))) {
        throw "visual comparison did not emit the difference image"
    }
}
finally {
    if (Test-Path $root) { Remove-Item -Recurse -Force $root }
}
