<#
Install or update Foton on Windows.

    irm https://foton.zeffut.fr/install.ps1 | iex
    iex "& { $(irm https://foton.zeffut.fr/install.ps1) } -Update"

Read-Host reads the console directly on Windows, so prompts still work under
`irm | iex` even though standard input is consumed by the pipe (unlike the
Unix installer, which has to read /dev/tty for exactly that reason). When the
host has no console at all -- a scheduled task, a CI runner -- every question
takes its default and says so, once, before continuing.

This targets Windows PowerShell 5.1, which is what ships with Windows and
what most people invoking this one-liner actually have. No ??, no ternaries,
no -AsHashtable, no &&/||, nothing that only exists in PowerShell 7.
#>

[CmdletBinding()]
param(
    # Matches the shell installer's `--update`: replaces the binary in the
    # current directory, leaves config\ and saves\ alone, and does nothing
    # when the installed version already matches the latest release.
    [switch]$Update
)

$ErrorActionPreference = 'Stop'

# GitHub refuses TLS 1.0, which is still the default for Invoke-WebRequest on
# an unpatched Windows install running PowerShell 5.1. Without this the
# installer fails on precisely the machines that need it most.
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

# Invoke-WebRequest's progress bar makes a ~70 MB download roughly an order of
# magnitude slower under Windows PowerShell 5.1.
$ProgressPreference = 'SilentlyContinue'

$Repo = 'Zeffut/Foton'
$Api = "https://api.github.com/repos/$Repo/releases/latest"

# The installed binary's name. Every reference to it below goes through this
# variable, mirroring install.sh's $BIN.
$Bin = 'foton.exe'

function Write-Bold {
    param([string]$Text)
    Write-Host $Text -ForegroundColor Cyan
}

# Throws a plain message; the top-level catch below prints it as a sentence
# instead of a PowerShell stack trace.
function Die {
    param([string]$Message)
    throw $Message
}

function Test-Interactive {
    if (-not [Environment]::UserInteractive) { return $false }
    try {
        $null = $Host.UI.RawUI.WindowSize
        return $true
    } catch {
        return $false
    }
}

# Read-Answer <prompt> <default> -- returns the answer, or the default when
# the host is non-interactive or the reply is empty.
function Read-Answer {
    param([string]$Prompt, [string]$Default)
    if (-not $script:Interactive) { return $Default }
    $reply = Read-Host "$Prompt [$Default]"
    if ([string]::IsNullOrEmpty($reply)) { return $Default }
    return $reply
}

# Pulls an HTTP status code out of a WebException (Windows PowerShell 5.1) or
# an HttpResponseException (PowerShell 7), without depending on which one was
# thrown -- both expose a Response whose StatusCode casts to an int.
function Get-HttpStatusCode {
    param($ErrorRecord)
    $response = $ErrorRecord.Exception.Response
    if (-not $response) { return $null }
    try { return [int]$response.StatusCode } catch { return $null }
}

# Set-TomlKey <path> <key> <value> -- rewrites `^key = ...` in place, the
# PowerShell equivalent of install.sh's `sed -i.bak "s|^$2 *=.*|$2 = $3|"`.
# A MatchEvaluator (not a plain -replace string) sidesteps .NET regex
# replacement syntax ($1, $&, ...) misfiring if a value ever contains a `$`.
function Set-TomlKey {
    param([string]$Path, [string]$Key, [string]$Value)
    if (-not (Test-Path $Path)) { return }
    $content = [System.IO.File]::ReadAllText($Path, [System.Text.Encoding]::UTF8)
    $pattern = '(?m)^' + [regex]::Escape($Key) + '\s*=.*$'
    if ($content -notmatch $pattern) { return }
    $evaluator = { param($m) "$Key = $Value" }
    $content = [regex]::Replace($content, $pattern, $evaluator)
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($Path, $content, $utf8NoBom)
}

$TempDir = $null

try {
    Write-Bold 'Foton installer'

    # Foton publishes no Windows ARM build. $env:PROCESSOR_ARCHITECTURE
    # reports ARM64 there, and x86 on 32-bit Windows -- neither is buildable
    # today, so both stop here rather than fail later on a missing asset.
    switch ($env:PROCESSOR_ARCHITECTURE) {
        'AMD64' { $Asset = 'foton-windows-x86_64.exe' }
        'ARM64' { Die "Foton publishes no Windows ARM build yet. Build from source instead: https://github.com/$Repo" }
        default { Die "unsupported processor: $($env:PROCESSOR_ARCHITECTURE). Foton publishes Windows x86_64 builds only." }
    }

    $script:Interactive = Test-Interactive

    Write-Host 'Looking up the latest release...'
    $Release = $null
    try {
        $Release = Invoke-RestMethod -Uri $Api -UseBasicParsing -ErrorAction Stop
    } catch {
        $status = Get-HttpStatusCode $_
        if ($status -eq 404) {
            Die "Foton has no published release yet. Build from source instead: https://github.com/$Repo"
        } elseif ($status) {
            Die "the GitHub API answered $status; try again in a moment"
        } else {
            Die 'could not reach the GitHub API -- check the network and try again'
        }
    }

    $Tag = $Release.tag_name
    if ([string]::IsNullOrEmpty($Tag)) { Die 'no published release yet' }
    # A JSON array with exactly one element unwraps to a scalar object rather
    # than a one-item array in Windows PowerShell 5.1's ConvertFrom-Json (and
    # so, transitively, in what Invoke-RestMethod hands back); @() guards
    # every place this list gets filtered or counted.
    $Assets = @($Release.assets)

    Write-Host "Latest release: $Tag"
    Write-Host "Asset for this machine: $Asset"

    $AssetInfo = $Assets | Where-Object { $_.name -eq $Asset } | Select-Object -First 1
    if (-not $AssetInfo) {
        Die "the $Tag release does not include a Windows build yet -- it currently ships macOS and Linux only. Watch https://github.com/$Repo/releases or build from source."
    }
    $SumsInfo = $Assets | Where-Object { $_.name -eq 'SHA256SUMS' } | Select-Object -First 1
    if (-not $SumsInfo) {
        Die "SHA256SUMS is missing from the $Tag release"
    }

    if ($Update) {
        if (-not (Test-Path ".\$Bin")) {
            Die '-Update must run inside an existing installation'
        }
        $versionOutput = & ".\$Bin" --version 2>$null
        $current = (($versionOutput -join ' ') -split '\s+')[1]
        if ("v$current" -eq $Tag) {
            Write-Bold "Already on $Tag. Nothing to do."
            exit 0
        }
        Write-Host "Updating from $current to $Tag"
        $Dir = '.'
    } else {
        $Dir = '.'
        if (Test-Path (Join-Path $Dir $Bin)) {
            $overwrite = Read-Answer 'This directory already has Foton. Replace the binary?' 'no'
            if ($overwrite -notmatch '^(y|yes)$') {
                Die 'stopping, nothing was changed'
            }
        }
    }

    $TempDir = Join-Path $env:TEMP ('foton-install-' + [Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Force -Path $TempDir | Out-Null
    $tempBin = Join-Path $TempDir $Bin
    $tempSums = Join-Path $TempDir 'SHA256SUMS'

    Write-Host 'Downloading...'
    try {
        Invoke-WebRequest -Uri $AssetInfo.browser_download_url -OutFile $tempBin -UseBasicParsing -ErrorAction Stop
    } catch {
        Die "could not download $Asset from $Tag"
    }
    try {
        Invoke-WebRequest -Uri $SumsInfo.browser_download_url -OutFile $tempSums -UseBasicParsing -ErrorAction Stop
    } catch {
        Die 'could not download SHA256SUMS'
    }

    Write-Host 'Verifying...'
    $sumsLines = Get-Content -Path $tempSums
    $expected = $null
    $escapedAsset = [regex]::Escape($Asset)
    foreach ($line in $sumsLines) {
        if ($line -match "^\s*([0-9a-fA-F]{64})\s+\*?$escapedAsset\s*$") {
            $expected = $Matches[1]
            break
        }
    }
    if (-not $expected) { Die "$Asset is not listed in SHA256SUMS" }
    # Get-FileHash returns uppercase hex; shasum/sha256sum write lowercase.
    # Compare case-insensitively rather than relying on -eq's default
    # case-insensitivity, so the intent reads plainly at the call site.
    $actual = (Get-FileHash -Algorithm SHA256 -Path $tempBin).Hash
    if ($expected.ToUpperInvariant() -ne $actual.ToUpperInvariant()) {
        Remove-Item -Force $tempBin -ErrorAction SilentlyContinue
        Die 'checksum mismatch -- the download does not match the published release'
    }

    $finalBin = Join-Path $Dir $Bin
    if (Test-Path $finalBin) {
        $previousBin = Join-Path $Dir "$Bin.previous"
        Move-Item -Force $finalBin $previousBin
    }
    Move-Item -Force $tempBin $finalBin
    Write-Bold "Installed $Tag to .\$Bin"

    if ($Update) {
        Write-Bold 'Updated. Your config\ and saves\ were left alone.'
        exit 0
    }

    Write-Host 'Writing the default configuration...'
    Push-Location $Dir
    try {
        & ".\$Bin" --generate-config
        if ($LASTEXITCODE -ne 0) { Die 'could not generate the configuration' }
    } finally {
        Pop-Location
    }

    if (-not $script:Interactive) {
        Write-Bold 'No terminal here, so the defaults were kept. Edit .\config\ to change them.'
        exit 0
    }

    $name = Read-Answer 'Server name' 'A Foton Server'
    $port = Read-Answer 'Port' '25565'
    $players = Read-Answer 'Maximum players' '20'
    $online = Read-Answer 'Require a Mojang account to join?' 'yes'
    $difficulty = Read-Answer 'Difficulty (peaceful, easy, normal, hard)' 'normal'

    if ($online -match '^(n|no)$') { $onlineValue = 'false' } else { $onlineValue = 'true' }

    Set-TomlKey (Join-Path $Dir 'config\config.toml') 'motd' ('"' + $name + '"')
    Set-TomlKey (Join-Path $Dir 'config\config.toml') 'server_port' $port
    Set-TomlKey (Join-Path $Dir 'config\config.toml') 'max_players' $players
    Set-TomlKey (Join-Path $Dir 'config\config.toml') 'online_mode' $onlineValue
    Set-TomlKey (Join-Path $Dir 'config\worlds.toml') 'difficulty' ('"' + $difficulty + '"')

    Write-Bold 'Done.'
    Write-Host "Start it with:  .\$Bin"
    $start = Read-Answer 'Start it now?' 'yes'
    if ($start -match '^(y|yes)$') {
        Set-Location $Dir
        & ".\$Bin"
    }
} catch {
    Write-Host "error: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
} finally {
    if ($TempDir -and (Test-Path $TempDir)) {
        Remove-Item -Recurse -Force $TempDir -ErrorAction SilentlyContinue
    }
}
