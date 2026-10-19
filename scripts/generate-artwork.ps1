# Regenerate the committed artwork on Windows. Packaging consumes these files
# directly, so release runners do not need a graphics tool or font installation.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$taskRoot = Split-Path -Parent $PSScriptRoot
$taskOutput = Join-Path $taskRoot 'packaging/icons'
New-Item -ItemType Directory -Path $taskOutput -Force | Out-Null
[xml]$taskSvg = Get-Content -LiteralPath (Join-Path $taskRoot 'packaging/cult-copy.svg') -Raw

function New-RoundedPath([single]$X, [single]$Y, [single]$Width, [single]$Height, [single]$Radius) {
    $taskPath = [Drawing.Drawing2D.GraphicsPath]::new()
    $taskDiameter = $Radius * 2
    $taskPath.AddArc($X, $Y, $taskDiameter, $taskDiameter, 180, 90)
    $taskPath.AddArc(($X+$Width-$taskDiameter), $Y, $taskDiameter, $taskDiameter, 270, 90)
    $taskPath.AddArc(($X+$Width-$taskDiameter), ($Y+$Height-$taskDiameter), $taskDiameter, $taskDiameter, 0, 90)
    $taskPath.AddArc($X, ($Y+$Height-$taskDiameter), $taskDiameter, $taskDiameter, 90, 90)
    $taskPath.CloseFigure()
    return ,$taskPath
}

function New-BrandBitmap([int]$Size, [bool]$Tray = $false) {
    $taskBitmap = [Drawing.Bitmap]::new($Size, $Size)
    $taskGraphics = [Drawing.Graphics]::FromImage($taskBitmap)
    try {
        $taskGraphics.SmoothingMode = [Drawing.Drawing2D.SmoothingMode]::AntiAlias
        $taskGraphics.ScaleTransform(($Size/128.0), ($Size/128.0))
        $taskIndex = 0
        foreach ($taskRect in $taskSvg.svg.rect) {
            $taskIndex++
            if ($Tray -and $taskIndex -eq 1) { continue }
            $taskX = 0; $taskY = 0
            if ($taskRect.x) { $taskX = [single]$taskRect.x }
            if ($taskRect.y) { $taskY = [single]$taskRect.y }
            $taskPath = New-RoundedPath $taskX $taskY ([single]$taskRect.width) ([single]$taskRect.height) ([single]$taskRect.rx)
            try {
                if ($taskRect.fill -ne 'none') {
                    $taskColor = [Drawing.ColorTranslator]::FromHtml($taskRect.fill)
                    if ($Tray) {
                        $taskColor = [Drawing.Color]::Transparent
                        $taskGraphics.CompositingMode = [Drawing.Drawing2D.CompositingMode]::SourceCopy
                    }
                    $taskBrush = [Drawing.SolidBrush]::new($taskColor)
                    try { $taskGraphics.FillPath($taskBrush, $taskPath) } finally { $taskBrush.Dispose() }
                    $taskGraphics.CompositingMode = [Drawing.Drawing2D.CompositingMode]::SourceOver
                }
                if ($taskRect.stroke) {
                    $taskPen = [Drawing.Pen]::new([Drawing.ColorTranslator]::FromHtml($taskRect.stroke), [single]$taskRect.'stroke-width')
                    try { $taskGraphics.DrawPath($taskPen, $taskPath) } finally { $taskPen.Dispose() }
                }
            } finally { $taskPath.Dispose() }
        }
    } finally { $taskGraphics.Dispose() }
    return ,$taskBitmap
}

function Get-PngBytes([int]$Size) {
    $taskBitmap = New-BrandBitmap $Size
    $taskStream = [IO.MemoryStream]::new()
    try {
        $taskBitmap.Save($taskStream, [Drawing.Imaging.ImageFormat]::Png)
        return ,$taskStream.ToArray()
    } finally { $taskBitmap.Dispose(); $taskStream.Dispose() }
}
foreach ($taskSize in @(16,32,48,64,128,256,512,1024)) {
    [IO.File]::WriteAllBytes((Join-Path $taskOutput "icon-$taskSize.png"), (Get-PngBytes $taskSize))
}
$taskTray = New-BrandBitmap 32 $true
try { $taskTray.Save((Join-Path $taskOutput 'tray-32.png'), [Drawing.Imaging.ImageFormat]::Png) } finally { $taskTray.Dispose() }

$taskStream = [IO.File]::Create((Join-Path $taskOutput 'cult-copy.ico'))
$taskWriter = [IO.BinaryWriter]::new($taskStream)
try {
    $taskSizes = @(16,32,48,64,128,256)
    $taskWriter.Write([uint16]0); $taskWriter.Write([uint16]1); $taskWriter.Write([uint16]$taskSizes.Count)
    $taskOffset = 6 + 16 * $taskSizes.Count
    foreach ($taskSize in $taskSizes) {
        $taskPng = [IO.File]::ReadAllBytes((Join-Path $taskOutput "icon-$taskSize.png"))
        $taskDimension = if ($taskSize -eq 256) { 0 } else { $taskSize }
        $taskWriter.Write([byte]$taskDimension); $taskWriter.Write([byte]$taskDimension)
        $taskWriter.Write([uint16]0); $taskWriter.Write([uint16]1); $taskWriter.Write([uint16]32)
        $taskWriter.Write([uint32]$taskPng.Length); $taskWriter.Write([uint32]$taskOffset)
        $taskOffset += $taskPng.Length
    }
    foreach ($taskSize in $taskSizes) { $taskWriter.Write([IO.File]::ReadAllBytes((Join-Path $taskOutput "icon-$taskSize.png"))) }
} finally { $taskWriter.Dispose() }

function Write-BigEndian([IO.BinaryWriter]$Writer, [int]$Value) {
    $taskBytes = [BitConverter]::GetBytes($Value)
    if ([BitConverter]::IsLittleEndian) { [Array]::Reverse($taskBytes) }
    $Writer.Write($taskBytes)
}
$taskStream = [IO.MemoryStream]::new()
$taskWriter = [IO.BinaryWriter]::new($taskStream)
try {
    $taskWriter.Write([Text.Encoding]::ASCII.GetBytes('icns')); Write-BigEndian $taskWriter 0
    foreach ($taskEntry in @(@('icp4',16),@('icp5',32),@('icp6',64),@('ic07',128),@('ic08',256),@('ic09',512),@('ic10',1024))) {
        $taskPng = [IO.File]::ReadAllBytes((Join-Path $taskOutput ('icon-' + $taskEntry[1] + '.png')))
        $taskWriter.Write([Text.Encoding]::ASCII.GetBytes($taskEntry[0])); Write-BigEndian $taskWriter ($taskPng.Length+8); $taskWriter.Write($taskPng)
    }
    $taskLength = [int]$taskStream.Length
    $taskStream.Position = 4; Write-BigEndian $taskWriter $taskLength
    [IO.File]::WriteAllBytes((Join-Path $taskOutput 'cult-copy.icns'), $taskStream.ToArray())
} finally { $taskWriter.Dispose() }

# GitHub's recommended 1280x640 social-preview size. Upload once in repository
# Settings > Social preview; committing this file alone does not set that field.
$taskCard = [Drawing.Bitmap]::new(1280,640)
$taskGraphics = [Drawing.Graphics]::FromImage($taskCard)
$taskBrand = New-BrandBitmap 240
try {
    $taskGraphics.Clear([Drawing.ColorTranslator]::FromHtml('#121722'))
    $taskGraphics.TextRenderingHint = [Drawing.Text.TextRenderingHint]::AntiAliasGridFit
    $taskGraphics.DrawImage($taskBrand, 72, 114, 240, 240)
    $taskTitle = [Drawing.Font]::new('Segoe UI', 52, [Drawing.FontStyle]::Bold, [Drawing.GraphicsUnit]::Pixel)
    $taskBody = [Drawing.Font]::new('Segoe UI', 28, [Drawing.FontStyle]::Regular, [Drawing.GraphicsUnit]::Pixel)
    $taskSmall = [Drawing.Font]::new('Segoe UI', 22, [Drawing.FontStyle]::Regular, [Drawing.GraphicsUnit]::Pixel)
    $taskWhite = [Drawing.SolidBrush]::new([Drawing.Color]::White)
    $taskGreen = [Drawing.SolidBrush]::new([Drawing.ColorTranslator]::FromHtml('#6de0c1'))
    $taskMuted = [Drawing.SolidBrush]::new([Drawing.ColorTranslator]::FromHtml('#97a3b8'))
    try {
        $taskGraphics.DrawString('Cult Copy', $taskTitle, $taskWhite, 356, 144)
        $taskGraphics.DrawString('One clipboard across your computers.', $taskBody, $taskWhite, 360, 232)
        $taskGraphics.DrawString('Text, images & files. Paired. Local. Private.', $taskBody, $taskMuted, 360, 282)
        $taskGraphics.DrawString('WINDOWS  /  macOS  /  LINUX', $taskSmall, $taskGreen, 80, 454)
        $taskGraphics.DrawString('Independent open-source project. No affiliation with Duet Display.', $taskSmall, $taskMuted, 80, 518)
        $taskCard.Save((Join-Path $taskRoot 'docs/social-preview.png'), [Drawing.Imaging.ImageFormat]::Png)
    } finally { $taskTitle.Dispose(); $taskBody.Dispose(); $taskSmall.Dispose(); $taskWhite.Dispose(); $taskGreen.Dispose(); $taskMuted.Dispose() }
} finally { $taskGraphics.Dispose(); $taskBrand.Dispose(); $taskCard.Dispose() }
Write-Output 'Generated app, tray, Windows, Mac, and social-preview artwork from packaging/cult-copy.svg.'
