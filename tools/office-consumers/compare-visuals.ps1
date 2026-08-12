param(
    [Parameter(Mandatory = $true)][string]$ExpectedDirectory,
    [Parameter(Mandatory = $true)][string]$ActualDirectory,
    [Parameter(Mandatory = $true)][string]$ReportPath,
    [double]$MaximumDifferentPixelRatio = 0.35
)

$ErrorActionPreference = "Stop"
$nativeErrorPreferenceWasSet = Test-Path variable:PSNativeCommandUseErrorActionPreference
if ($nativeErrorPreferenceWasSet) {
    $previousNativeErrorPreference = $PSNativeCommandUseErrorActionPreference
    $PSNativeCommandUseErrorActionPreference = $false
}
$slides = @()
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
}
finally {
    if ($nativeErrorPreferenceWasSet) {
        $PSNativeCommandUseErrorActionPreference = $previousNativeErrorPreference
    }
}
$report = [ordered]@{ schema = 1; source = "PowerPoint"; slides = $slides }
$report | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 $ReportPath
if ($slides.Where({ !$_.passed }).Count -ne 0) {
    throw "PowerPoint visual difference exceeded the declared tolerance"
}
