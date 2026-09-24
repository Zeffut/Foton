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
$RuntimeAsset = 'foton-plugin-runtime.zip'

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
    if ($env:FOTON_INSTALL_NONINTERACTIVE -eq '1') { return $false }
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

function Test-RegularFile {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $false }
    $item = Get-Item -LiteralPath $Path -Force
    return -not (($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0)
}

function Test-DirectDirectory {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path -PathType Container)) { return $false }
    $item = Get-Item -LiteralPath $Path -Force
    return -not (($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0)
}

function Assert-SafeTransaction {
    param([string]$Path)
    if (-not (Test-DirectDirectory $Path)) { throw "$Path is not a direct transaction directory" }
    $owner = Join-Path $Path 'owner'
    if (-not (Test-RegularFile $owner) -or
        ([System.IO.File]::ReadAllText($owner).Trim()) -notmatch '^\d+:[0-9a-f]{32}$') {
        throw "$Path has an invalid ownership marker"
    }
    $allowedMarkers = @('owner', 'committed', 'replace-binary-started', 'replace-runtime-started')
    $allowedBinaries = @('new-binary', 'previous-binary')
    $allowedDirectories = @('new-runtime', 'previous-runtime')
    foreach ($entry in @(Get-ChildItem -LiteralPath $Path -Force)) {
        if ($entry.Name -in $allowedMarkers) {
            if (-not (Test-RegularFile $entry.FullName)) { throw "unsafe transaction file $($entry.FullName)" }
            if ($entry.Name -ne 'owner' -and $entry.Length -ne 0) { throw "non-empty transaction marker $($entry.FullName)" }
        } elseif ($entry.Name -in $allowedBinaries) {
            if (-not (Test-RegularFile $entry.FullName)) { throw "unsafe transaction binary $($entry.FullName)" }
        } elseif ($entry.Name -in $allowedDirectories) {
            if (-not (Test-DirectDirectory $entry.FullName)) { throw "unsafe transaction directory $($entry.FullName)" }
            $pending = New-Object 'System.Collections.Generic.Stack[string]'
            $pending.Push($entry.FullName)
            while ($pending.Count -gt 0) {
                $current = $pending.Pop()
                foreach ($child in @(Get-ChildItem -LiteralPath $current -Force)) {
                    if (($child.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
                        throw "transaction runtime contains a reparse point: $($child.FullName)"
                    }
                    if ($child.PSIsContainer) { $pending.Push($child.FullName) }
                    elseif (-not (Test-RegularFile $child.FullName)) { throw "transaction runtime contains a special file: $($child.FullName)" }
                }
            }
        } else {
            throw "unexpected transaction entry $($entry.FullName)"
        }
    }
}

function Test-RuntimeComplete {
    param([string]$Path, [string]$Tag, [bool]$RequireReleaseTag = $true)
    $api = Join-Path $Path 'foton-plugin-api.jar'
    $libraries = Join-Path $Path 'lib'
    $manifest = Join-Path $Path 'SHA256SUMS'
    $releaseTag = Join-Path $Path '.release-tag'
    $licenses = Join-Path $Path 'licenses'
    if (-not (Test-RegularFile $api) -or
        -not (Test-Path -LiteralPath $libraries -PathType Container) -or
        ((Get-Item -LiteralPath $libraries -Force).Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0 -or
        -not (Test-Path -LiteralPath $licenses -PathType Container) -or
        ((Get-Item -LiteralPath $licenses -Force).Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0 -or
        -not (Test-RegularFile $manifest)) {
        return $false
    }
    if ($RequireReleaseTag) {
        if (-not (Test-RegularFile $releaseTag) -or
            ([System.IO.File]::ReadAllText($releaseTag).Trim()) -ne $Tag) { return $false }
        $expectedRootEntries = 5
    } else {
        if (Get-Item -LiteralPath $releaseTag -Force -ErrorAction SilentlyContinue) { return $false }
        $expectedRootEntries = 4
    }
    if (@(Get-ChildItem -LiteralPath $Path -Force).Count -ne $expectedRootEntries) { return $false }

    $expectedLibraries = @(
        'adventure-api-5.2.0.jar',
        'adventure-key-5.2.0.jar',
        'adventure-text-logger-slf4j-5.2.0.jar',
        'adventure-text-serializer-plain-5.2.0.jar',
        'annotations-26.1.0.jar',
        'brigadier-1.3.10.jar',
        'error_prone_annotations-2.47.0.jar',
        'failureaccess-1.0.3.jar',
        'gson-2.14.0.jar',
        'guava-33.6.0-jre.jar',
        'j2objc-annotations-3.1.jar',
        'joml-1.10.8.jar',
        'jspecify-1.0.0.jar',
        'kotlin-stdlib-1.8.20.jar',
        'kotlin-stdlib-common-1.8.20.jar',
        'kotlin-stdlib-jdk7-1.8.20.jar',
        'kotlin-stdlib-jdk8-1.8.20.jar',
        'netty-buffer-4.2.15.Final.jar',
        'netty-codec-base-4.2.15.Final.jar',
        'netty-common-4.2.15.Final.jar',
        'netty-resolver-4.2.15.Final.jar',
        'netty-transport-4.2.15.Final.jar',
        'slf4j-api-2.0.17.jar',
        'snakeyaml-2.2.jar'
    )

    $apiSeen = 0
    $librariesSeen = 0
    $licensesSeen = 0
    $manifestEntries = 0
    $seenPaths = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::Ordinal)
    foreach ($line in Get-Content -Path $manifest) {
        if ([string]::IsNullOrWhiteSpace($line)) { continue }
        if ($line -notmatch '^([0-9a-fA-F]{64})\s+\*?(.+)$') { return $false }
        $expected = $Matches[1]
        $relative = $Matches[2]
        if (-not $seenPaths.Add($relative)) { return $false }
        if ($relative -eq 'foton-plugin-api.jar') {
            $apiSeen++
        } elseif ($relative -match '^lib/([^/]+\.jar)$' -and $Matches[1] -cin $expectedLibraries) {
            $librariesSeen++
        } elseif ($relative -match '^licenses/[A-Za-z0-9._+-]+$') {
            $licensesSeen++
        } else {
            return $false
        }
        $file = Join-Path $Path ($relative -replace '/', '\')
        if (-not (Test-RegularFile $file)) { return $false }
        $actual = (Get-FileHash -Algorithm SHA256 -Path $file).Hash
        if ($actual.ToUpperInvariant() -ne $expected.ToUpperInvariant()) { return $false }
        $manifestEntries++
    }
    $actualFiles = @(Get-ChildItem -Path $Path -Recurse -File | Where-Object {
        $_.Name -ne 'SHA256SUMS' -and $_.Name -ne '.release-tag'
    })
    $libraryEntries = @(Get-ChildItem -LiteralPath $libraries -Force)
    if (@($libraryEntries | Where-Object {
        $_.PSIsContainer -or (($_.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0)
    }).Count -gt 0) { return $false }
    $libraryFiles = @($libraryEntries | Where-Object { -not $_.PSIsContainer })
    $jarFiles = @($libraryFiles | Where-Object { $_.Extension -eq '.jar' })
    $actualLibraryNames = @($jarFiles | ForEach-Object { $_.Name } | Sort-Object)
    $sortedExpectedLibraries = @($expectedLibraries | Sort-Object)
    if (($actualLibraryNames -join "`n") -cne ($sortedExpectedLibraries -join "`n")) { return $false }
    $expectedLicenses = @(
        'ADVENTURE-MIT.txt', 'APACHE-2.0.txt', 'BRIGADIER-MIT.txt',
        'JOML-MIT.txt', 'SLF4J-MIT.txt', 'THIRD-PARTY-NOTICES.txt'
    )
    $licenseEntries = @(Get-ChildItem -LiteralPath $licenses -Force)
    if ($licenseEntries.Count -ne $expectedLicenses.Count) { return $false }
    foreach ($licenseName in $expectedLicenses) {
        if (-not (Test-RegularFile (Join-Path $licenses $licenseName))) { return $false }
    }
    return $apiSeen -eq 1 -and $librariesSeen -eq 24 -and
        $jarFiles.Count -eq 24 -and $libraryFiles.Count -eq 24 -and
        $licensesSeen -eq 6 -and $manifestEntries -eq $actualFiles.Count
}

function Restore-PendingTransaction {
    param([string]$Path, [string]$InstallDir, [string]$BinaryName)
    if (-not (Test-Path $Path)) { return $false }
    Assert-SafeTransaction $Path
    if (Test-Path (Join-Path $Path 'committed')) {
        Remove-Item -Recurse -Force $Path
        return $false
    }

    $errors = New-Object System.Collections.Generic.List[string]
    $finalBinary = Join-Path $InstallDir $BinaryName
    $finalRuntime = Join-Path $InstallDir 'plugin-runtime'
    $previousBinary = Join-Path $Path 'previous-binary'
    $previousRuntime = Join-Path $Path 'previous-runtime'
    if ((Test-Path $previousRuntime) -or (Test-Path (Join-Path $Path 'replace-runtime-started'))) {
        if (Test-Path $finalRuntime) {
            try { Remove-Item -Recurse -Force $finalRuntime } catch { $errors.Add('current runtime') }
        }
        if ((Test-Path $previousRuntime) -and -not (Test-Path $finalRuntime)) {
            try { Copy-Item -Recurse -Force $previousRuntime $finalRuntime } catch { $errors.Add("runtime backup $previousRuntime") }
        }
    }
    if ((Test-Path $previousBinary) -or (Test-Path (Join-Path $Path 'replace-binary-started'))) {
        if (Test-Path $finalBinary) {
            try { Remove-Item -Force $finalBinary } catch { $errors.Add('current binary') }
        }
        if ((Test-Path $previousBinary) -and -not (Test-Path $finalBinary)) {
            try { Copy-Item -Force $previousBinary $finalBinary } catch { $errors.Add("binary backup $previousBinary") }
        }
    }
    if ($errors.Count -gt 0) {
        throw "rollback was incomplete: $($errors -join ', ')"
    }
    Remove-Item -Recurse -Force $Path
    return $true
}

function Invoke-FotonInstaller {
    $TempDir = $null
    $StagedBin = $null
    $StagedRuntime = $null
    $LockStream = $null
    $LockOwned = $false
    $LockPath = $null
    $TransactionDir = $null
    $Dir = '.'

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

    $LockPath = Join-Path $Dir '.foton-install.lock'
    $existingLock = Get-Item -LiteralPath $LockPath -Force -ErrorAction SilentlyContinue
    if ($existingLock -and ($existingLock.PSIsContainer -or
        (($existingLock.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0))) {
        Die "$LockPath is not a direct regular lock file"
    }
    try {
        $LockStream = [System.IO.File]::Open(
            $LockPath,
            [System.IO.FileMode]::OpenOrCreate,
            [System.IO.FileAccess]::ReadWrite,
            [System.IO.FileShare]::None
        )
    } catch {
        Die "another installer is already modifying $Dir"
    }
    $LockOwned = $true
    $ownerBytes = [System.Text.Encoding]::UTF8.GetBytes("$PID`n")
    $LockStream.SetLength(0)
    $LockStream.Write($ownerBytes, 0, $ownerBytes.Length)
    $LockStream.Flush()

    $TransactionDir = Join-Path $Dir '.foton-install-transaction'
    if ((Get-Item -LiteralPath $TransactionDir -Force -ErrorAction SilentlyContinue)) {
        $recovered = Restore-PendingTransaction $TransactionDir $Dir $Bin
        if ($recovered) { Write-Bold 'Recovered an interrupted installation before continuing.' }
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
    $RuntimeInfo = $Assets | Where-Object { $_.name -eq $RuntimeAsset } | Select-Object -First 1
    if (-not $RuntimeInfo) {
        Die "$RuntimeAsset is missing from the $Tag release"
    }

    if ($Update) {
        if (-not (Test-Path ".\$Bin")) {
            Die '-Update must run inside an existing installation'
        }
        $versionOutput = & ".\$Bin" --version 2>$null
        $current = (($versionOutput -join ' ') -split '\s+')[1]
        if (("v$current" -eq $Tag) -and (Test-RuntimeComplete '.\plugin-runtime' $Tag)) {
            Write-Bold "Already on $Tag. Nothing to do."
            return
        }
        if ("v$current" -eq $Tag) {
            Write-Host "Repairing the plugin runtime for $Tag"
        } else {
            Write-Host "Updating from $current to $Tag"
        }
        $Dir = '.'
    } else {
        $Dir = '.'
        if ((Test-Path (Join-Path $Dir $Bin)) -or (Test-Path (Join-Path $Dir 'plugin-runtime'))) {
            $overwrite = Read-Answer 'This directory already has a Foton binary or plugin runtime. Replace the binary/runtime pair? Config, plugins and saves stay untouched.' 'no'
            if ($overwrite -notmatch '^(y|yes)$') {
                Die 'stopping, nothing was changed'
            }
        }
    }

    $TempDir = Join-Path $env:TEMP ('foton-install-' + [Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Force -Path $TempDir | Out-Null
    $tempBin = Join-Path $TempDir $Bin
    $tempRuntime = Join-Path $TempDir $RuntimeAsset
    $tempSums = Join-Path $TempDir 'SHA256SUMS'

    Write-Host 'Downloading...'
    try {
        Invoke-WebRequest -Uri $AssetInfo.browser_download_url -OutFile $tempBin -UseBasicParsing -ErrorAction Stop
    } catch {
        Die "could not download $Asset from $Tag"
    }
    try {
        Invoke-WebRequest -Uri $RuntimeInfo.browser_download_url -OutFile $tempRuntime -UseBasicParsing -ErrorAction Stop
    } catch {
        Die "could not download $RuntimeAsset from $Tag"
    }
    try {
        Invoke-WebRequest -Uri $SumsInfo.browser_download_url -OutFile $tempSums -UseBasicParsing -ErrorAction Stop
    } catch {
        Die 'could not download SHA256SUMS'
    }

    Write-Host 'Verifying...'
    $sumsLines = Get-Content -Path $tempSums
    foreach ($download in @(
        @{ Path = $tempBin; Name = $Asset },
        @{ Path = $tempRuntime; Name = $RuntimeAsset }
    )) {
        $expected = $null
        $escapedName = [regex]::Escape($download.Name)
        foreach ($line in $sumsLines) {
            if ($line -match "^\s*([0-9a-fA-F]{64})\s+\*?$escapedName\s*$") {
                $expected = $Matches[1]
                break
            }
        }
        if (-not $expected) { Die "$($download.Name) is not listed in SHA256SUMS" }
        # Get-FileHash returns uppercase hex; shasum/sha256sum write lowercase.
        $actual = (Get-FileHash -Algorithm SHA256 -Path $download.Path).Hash
        if ($expected.ToUpperInvariant() -ne $actual.ToUpperInvariant()) {
            Remove-Item -Force $download.Path -ErrorAction SilentlyContinue
            Die "checksum mismatch for $($download.Name) -- the download does not match the published release"
        }
    }

    $unpackedRuntime = Join-Path $TempDir 'plugin-runtime'
    Expand-Archive -Path $tempRuntime -DestinationPath $unpackedRuntime -Force
    if (-not (Test-Path (Join-Path $unpackedRuntime 'foton-plugin-api.jar'))) {
        Die "$RuntimeAsset has no plugin API jar"
    }
    $unpackedLibraries = Join-Path $unpackedRuntime 'lib'
    if (-not (Test-Path $unpackedLibraries) -or
        -not (Get-ChildItem -Path $unpackedLibraries -Filter '*.jar' -File | Select-Object -First 1)) {
        Die "$RuntimeAsset has no runtime library jars"
    }
    if (-not (Test-RuntimeComplete $unpackedRuntime $Tag $false)) {
        Die "$RuntimeAsset has an incomplete, unsafe or invalid runtime manifest"
    }
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText((Join-Path $unpackedRuntime '.release-tag'), "$Tag`n", $utf8NoBom)
    if (-not (Test-RuntimeComplete $unpackedRuntime $Tag)) {
        Die "$RuntimeAsset has an incomplete or invalid runtime manifest"
    }

    $finalBin = Join-Path $Dir $Bin
    $finalRuntime = Join-Path $Dir 'plugin-runtime'
    New-Item -ItemType Directory -Path $TransactionDir | Out-Null
    [System.IO.File]::WriteAllText(
        (Join-Path $TransactionDir 'owner'),
        "${PID}:$([Guid]::NewGuid().ToString('N'))`n",
        (New-Object System.Text.UTF8Encoding($false))
    )
    $StagedBin = Join-Path $TransactionDir 'new-binary'
    $StagedRuntime = Join-Path $TransactionDir 'new-runtime'
    $previousBin = Join-Path $TransactionDir 'previous-binary'
    $previousRuntime = Join-Path $TransactionDir 'previous-runtime'
    # The fixed transaction directory is both staging area and durable journal.
    # A later invocation can therefore recover after process termination.
    Move-Item -Force $tempBin $StagedBin
    New-Item -ItemType Directory -Path $StagedRuntime | Out-Null
    Copy-Item -Recurse -Force (Join-Path $unpackedRuntime '*') $StagedRuntime

    try {
        if (Test-Path $finalBin) {
            if (-not (Test-RegularFile $finalBin)) { throw "existing binary is not a direct regular file: $finalBin" }
            Move-Item -Force $finalBin $previousBin
        }
        if (Test-Path $finalRuntime) {
            if (-not (Test-DirectDirectory $finalRuntime)) { throw "existing plugin runtime is not a direct directory: $finalRuntime" }
            Move-Item -Force $finalRuntime $previousRuntime
        }
        New-Item -ItemType File -Path (Join-Path $TransactionDir 'replace-binary-started') | Out-Null
        Move-Item -Force $StagedBin $finalBin
        $StagedBin = $null
        New-Item -ItemType File -Path (Join-Path $TransactionDir 'replace-runtime-started') | Out-Null
        Move-Item -Force $StagedRuntime $finalRuntime
        $StagedRuntime = $null

        $installedVersionOutput = & $finalBin --version 2>$null
        if ($LASTEXITCODE -ne 0) { throw 'the new binary failed its version check' }
        $installedVersion = (($installedVersionOutput -join ' ') -split '\s+')[1]
        if ("v$installedVersion" -ne $Tag) {
            throw "the new binary reports version $installedVersion instead of $($Tag.Substring(1))"
        }
    } catch {
        $replacementError = $_.Exception.Message
        $hadPrevious = (Test-Path $previousBin) -or (Test-Path $previousRuntime)
        try {
            $null = Restore-PendingTransaction $TransactionDir $Dir $Bin
        } catch {
            throw "$replacementError; $($_.Exception.Message)"
        }
        if ($hadPrevious) {
            throw "$replacementError; the previous installation was restored"
        }
        throw "$replacementError; the incomplete new installation was removed"
    }
    New-Item -ItemType File -Path (Join-Path $TransactionDir 'committed') | Out-Null
    try {
        Remove-Item -Recurse -Force $TransactionDir
    } catch {
        Write-Host "warning: committed transaction cleanup remains at $TransactionDir" -ForegroundColor Yellow
    }
    Write-Bold "Installed $Tag to .\$Bin with the optional plugin runtime in .\plugin-runtime\"

    if ($Update) {
        Write-Bold 'Updated. Your config\, plugins\ and saves\ were left alone.'
        return
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
        return
    }

    $name = Read-Answer 'Server name' 'A Foton Server'
    $port = Read-Answer 'Port' '25565'
    $players = Read-Answer 'Maximum players' '20'
    $online = Read-Answer 'Require a Mojang account to join?' 'yes'
    $difficulty = Read-Answer 'Difficulty (peaceful, easy, normal, hard)' 'normal'

    if ($online -match '^(n|no)$') { $onlineValue = 'false' } else { $onlineValue = 'true' }

    if ([string]::IsNullOrWhiteSpace($name) -or $name -match '["\\\r\n]') {
        Die 'server name cannot be empty or contain quotes, backslashes, or line breaks'
    }
    $portNumber = 0
    if (-not [int]::TryParse($port, [ref]$portNumber) -or $portNumber -lt 1 -or $portNumber -gt 65000) {
        Die 'port must be a number from 1 to 65000'
    }
    $playerNumber = 0
    if (-not [int]::TryParse($players, [ref]$playerNumber) -or $playerNumber -lt 1) {
        Die 'maximum players must be a number from 1 to 2147483647'
    }
    if ($difficulty -notin @('peaceful', 'easy', 'normal', 'hard')) {
        Die 'difficulty must be peaceful, easy, normal, or hard'
    }

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
        $failureMessage = $_.Exception.Message
        Write-Host "error: $failureMessage" -ForegroundColor Red
        throw $failureMessage
    } finally {
        if ($TransactionDir -and (Test-Path $TransactionDir) -and
            -not (Test-Path (Join-Path $TransactionDir 'committed'))) {
            try {
                $null = Restore-PendingTransaction $TransactionDir $Dir $Bin
            } catch {
                Write-Host "error: interrupted installation; $($_.Exception.Message)" -ForegroundColor Red
            }
        }
        if ($TempDir -and (Test-Path $TempDir)) {
            Remove-Item -Recurse -Force $TempDir -ErrorAction SilentlyContinue
        }
        if ($StagedBin -and (Test-Path $StagedBin)) {
            Remove-Item -Force $StagedBin -ErrorAction SilentlyContinue
        }
        if ($StagedRuntime -and (Test-Path $StagedRuntime)) {
            Remove-Item -Recurse -Force $StagedRuntime -ErrorAction SilentlyContinue
        }
        if ($LockStream) {
            $LockStream.Dispose()
            $LockStream = $null
        }
        if ($LockOwned -and $LockPath -and (Test-Path $LockPath)) {
            Remove-Item -Force $LockPath -ErrorAction SilentlyContinue
        }
    }
}

Invoke-FotonInstaller
