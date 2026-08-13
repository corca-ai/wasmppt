param(
    [Parameter(Mandatory = $true)][string]$ExpectedDirectory,
    [Parameter(Mandatory = $true)][string]$ActualDirectory,
    [Parameter(Mandatory = $true)][string]$ReportPath,
    [double]$MaximumDifferentPixelRatio = 0.35,
    [string]$ManifestPath = "fixtures/render/powerpoint-baseline.json"
)

$ErrorActionPreference = "Stop"
$nativeErrorPreferenceWasSet = Test-Path variable:PSNativeCommandUseErrorActionPreference
if ($nativeErrorPreferenceWasSet) {
    $previousNativeErrorPreference = $PSNativeCommandUseErrorActionPreference
    $PSNativeCommandUseErrorActionPreference = $false
}
$slides = @()
$regions = @()
$manifest = Get-Content -Raw $ManifestPath | ConvertFrom-Json
$provenancePath = Join-Path (Split-Path $ExpectedDirectory -Parent) "provenance.json"
if (!(Test-Path $provenancePath)) { throw "PowerPoint provenance is missing" }
$provenance = Get-Content -Raw $provenancePath | ConvertFrom-Json
if ($provenance.fixtureSha256 -ne $manifest.fixtureSha256) { throw "PowerPoint fixture hash is stale" }
if ($provenance.exportWidth -ne $manifest.exportWidth -or $provenance.exportHeight -ne $manifest.exportHeight) {
    throw "PowerPoint export dimensions differ from the pinned manifest"
}
if (@($provenance.fonts).Count -eq 0) { throw "PowerPoint font inventory is empty" }
try {
    for ($index = 1; $index -le 2; $index++) {
        $expected = Join-Path $ExpectedDirectory "Slide$index.PNG"
        $actual = Join-Path $ActualDirectory "slide-$index-actual.png"
        $difference = Join-Path $ActualDirectory "slide-$index-difference.png"
        if (!(Test-Path $expected) -or !(Test-Path $actual)) {
            throw "missing expected or actual image for slide $index"
        }
        $dimensions = (& magick identify -format "%w %h" $expected).Split(" ")
        if ($LASTEXITCODE -ne 0) { throw "ImageMagick could not inspect slide $index" }
        $pixelCount = [double]$dimensions[0] * [double]$dimensions[1]
        # ImageMagick compare exits 1 when pixels differ; that is data, not a process failure.
        $metricOutput = & magick compare -metric AE -fuzz 5% $expected $actual $difference 2>&1
        if ($LASTEXITCODE -gt 1) { throw "ImageMagick failed while comparing slide $index" }
        $differentPixels = [double]($metricOutput | Select-Object -Last 1)
        $ratio = $differentPixels / $pixelCount
        $slides += [ordered]@{
            slideIndex = $index - 1
            expected = "Slide$index.PNG"
            actual = "slide-$index-actual.png"
            difference = "slide-$index-difference.png"
            differentPixels = $differentPixels
            pixelCount = $pixelCount
            differentPixelRatio = $ratio
            tolerance = $MaximumDifferentPixelRatio
            passed = $ratio -le $MaximumDifferentPixelRatio
        }
    }
    foreach ($region in $manifest.regions) {
        $slideNumber = [int]$region.slideIndex + 1
        $expected = Join-Path $ExpectedDirectory "Slide$slideNumber.PNG"
        $actual = Join-Path $ActualDirectory "slide-$slideNumber-actual.png"
        $prefix = "slide-$slideNumber-region-$($region.name)"
        $expectedCrop = Join-Path $ActualDirectory "$prefix-reference.png"
        $actualCrop = Join-Path $ActualDirectory "$prefix-actual.png"
        $difference = Join-Path $ActualDirectory "$prefix-difference.png"
        $geometry = "$($region.width)x$($region.height)+$($region.x)+$($region.y)"
        & magick $expected -crop $geometry +repage $expectedCrop
        if ($LASTEXITCODE -ne 0) { throw "ImageMagick could not crop reference region $($region.name)" }
        & magick $actual -crop $geometry +repage $actualCrop
        if ($LASTEXITCODE -ne 0) { throw "ImageMagick could not crop actual region $($region.name)" }
        $metricOutput = & magick compare -metric AE -fuzz 5% $expectedCrop $actualCrop $difference 2>&1
        if ($LASTEXITCODE -gt 1) { throw "ImageMagick failed for region $($region.name)" }
        $differentPixels = [double]($metricOutput | Select-Object -Last 1)
        $pixelCount = [double]$region.width * [double]$region.height
        $ratio = $differentPixels / $pixelCount
        $regions += [ordered]@{
            slideIndex = [int]$region.slideIndex
            name = $region.name
            geometry = $geometry
            expected = [IO.Path]::GetFileName($expectedCrop)
            actual = [IO.Path]::GetFileName($actualCrop)
            difference = [IO.Path]::GetFileName($difference)
            differentPixels = $differentPixels
            pixelCount = $pixelCount
            differentPixelRatio = $ratio
            tolerance = [double]$region.tolerance
            passed = $ratio -le [double]$region.tolerance
        }
    }
    $report = [ordered]@{
        schema = 2
        source = "PowerPoint"
        manifest = $manifest
        provenance = $provenance
        slides = $slides
        regions = $regions
    }
    $report | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 $ReportPath
    if ($slides.Where({ !$_.passed }).Count -ne 0 -or $regions.Where({ !$_.passed }).Count -ne 0) {
        throw "PowerPoint visual difference exceeded the declared tolerance"
    }
}
finally {
    if ($nativeErrorPreferenceWasSet) {
        $PSNativeCommandUseErrorActionPreference = $previousNativeErrorPreference
    }
}
