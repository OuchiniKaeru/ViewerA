param(
    [Parameter(Mandatory = $true)]
    [string[]] $Paths,

    [Parameter(Mandatory = $true)]
    [string] $Executable,

    [string] $AgentId = "powershell-agent",
    [string] $RunId,
    [string] $Message = "エージェントがファイルを生成しました。",
    [switch] $WaitForReceipt,
    [int] $ReceiptTimeoutMs = 10000,
    [int] $RetentionDays = 7
)

$ErrorActionPreference = "Stop"

if ($Paths.Count -lt 1 -or $Paths.Count -gt 20) {
    throw "Paths must contain between 1 and 20 files."
}
if ([string]::IsNullOrWhiteSpace($AgentId) -or $AgentId.Length -gt 128) {
    throw "AgentId must contain 1 to 128 characters."
}
if ([Text.Encoding]::UTF8.GetByteCount($Message) -gt 32768) {
    throw "Message exceeds 32 KiB."
}
if ($ReceiptTimeoutMs -le 0) {
    throw "ReceiptTimeoutMs must be positive."
}
if ($RetentionDays -lt 0) {
    throw "RetentionDays cannot be negative."
}

$dataDirectory = Join-Path $env:APPDATA "com.fileviewer.desktop"
$inboxDirectory = Join-Path $dataDirectory "agent-inbox"
$requestsDirectory = Join-Path $inboxDirectory "requests"
$receiptsDirectory = Join-Path $inboxDirectory "receipts"
foreach ($directory in @($dataDirectory, $inboxDirectory, $requestsDirectory, $receiptsDirectory)) {
    New-Item -ItemType Directory -Path $directory -Force | Out-Null
    $item = Get-Item -LiteralPath $directory -Force
    if (-not $item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw "Handoff path is not a regular directory: $directory"
    }
}

$cutoff = [DateTime]::UtcNow.AddDays(-$RetentionDays)
foreach ($directory in @($requestsDirectory, $receiptsDirectory)) {
    foreach ($oldFile in Get-ChildItem -LiteralPath $directory -File -Force) {
        if (
            $oldFile.Name -match '^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}(?:\.receipt)?\.json$' -and
            -not ($oldFile.Attributes -band [IO.FileAttributes]::ReparsePoint) -and
            $oldFile.LastWriteTimeUtc -lt $cutoff
        ) {
            Remove-Item -LiteralPath $oldFile.FullName -Force
        }
    }
}

$artifacts = @()
$seenPaths = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
foreach ($candidate in $Paths) {
    $resolvedPath = (Resolve-Path -LiteralPath $candidate -ErrorAction Stop).ProviderPath
    if (-not (Test-Path -LiteralPath $resolvedPath -PathType Leaf)) {
        throw "Artifact is not a regular file: $resolvedPath"
    }
    $file = Get-Item -LiteralPath $resolvedPath -Force
    if ($file.Attributes -band [IO.FileAttributes]::ReparsePoint) {
        throw "Symbolic links and reparse points are not accepted: $resolvedPath"
    }
    if ($seenPaths.Add($resolvedPath)) {
        $artifacts += @{
            artifact_id = [guid]::NewGuid().ToString()
            kind = "file"
            path = $resolvedPath
            name = $file.Name
            mime_type = "application/octet-stream"
            size = $file.Length
        }
    }
}
if ($artifacts.Count -lt 1 -or $artifacts.Count -gt 20) {
    throw "Paths must contain between 1 and 20 unique files."
}

$requestId = [guid]::NewGuid().ToString()
$payloadPath = Join-Path $requestsDirectory "$requestId.json"
$receiptPath = Join-Path $receiptsDirectory "$requestId.receipt.json"
$payload = @{
    schema_version = 1
    request_id = $requestId
    source = @{ platform = "powershell"; agent_id = $AgentId; run_id = $RunId }
    run = @{ status = "completed"; completed_at = [DateTime]::UtcNow.ToString("o") }
    viewer = @{
        action = "open_artifacts"
        selection = "first"
        focus = $true
        open_preview = $true
    }
    artifacts = $artifacts
}
if (-not [string]::IsNullOrEmpty($Message)) {
    $payload.result = @{ text = $Message; content_type = "text" }
}

$json = $payload | ConvertTo-Json -Depth 10 -Compress
$payloadBytes = [Text.Encoding]::UTF8.GetBytes($json)
if ($payloadBytes.Length -gt 1MB) {
    throw "Agent Return payload exceeds 1 MiB."
}
$temporaryPath = "$payloadPath.$([guid]::NewGuid().ToString('N')).tmp"
[IO.File]::WriteAllBytes($temporaryPath, $payloadBytes)
[IO.File]::Move($temporaryPath, $payloadPath)

$startInfo = [Diagnostics.ProcessStartInfo]::new()
$startInfo.FileName = $Executable
$startInfo.UseShellExecute = $false
$startInfo.CreateNoWindow = $true
foreach ($argument in @(
    "agent-return",
    "--payload", $payloadPath,
    "--receipt", $receiptPath,
    "--quiet"
)) {
    [void] $startInfo.ArgumentList.Add($argument)
}
$process = [Diagnostics.Process]::Start($startInfo)
if ($null -eq $process) {
    throw "ViewerA could not be started."
}

if ($WaitForReceipt) {
    $deadline = [DateTime]::UtcNow.AddMilliseconds($ReceiptTimeoutMs)
    while ([DateTime]::UtcNow -lt $deadline) {
        if (Test-Path -LiteralPath $receiptPath) {
            $receiptItem = Get-Item -LiteralPath $receiptPath -Force
            if (($receiptItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $receiptItem.Length -gt 1MB) {
                throw "ViewerA receipt failed safety checks."
            }
            try {
                $receipt = Get-Content -LiteralPath $receiptPath -Raw | ConvertFrom-Json
            } catch {
                $receipt = $null
            }
            if ($null -ne $receipt -and $receipt.status -in @("opened", "rejected", "failed")) {
                if ($receipt.status -ne "opened") {
                    throw "ViewerA returned $($receipt.status): $($receipt.errors -join '; ')"
                }
                $receipt
                return
            }
        }
        if ($process.HasExited -and $process.ExitCode -ne 0) {
            throw "ViewerA exited with status $($process.ExitCode) before opening the request."
        }
        Start-Sleep -Milliseconds 50
    }
    throw "ViewerA did not finish within $ReceiptTimeoutMs ms."
}

Write-Output "Sent Agent Return $requestId"
