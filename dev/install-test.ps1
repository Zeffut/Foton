<# Hermetic transaction checks for the Windows PowerShell installer. #>
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$Repo = Split-Path -Parent $PSScriptRoot
$Scratch = Join-Path $env:TEMP ('foton-install-test-' + [Guid]::NewGuid().ToString('N'))
$Assets = Join-Path $Scratch 'assets'
$Install = Join-Path $Scratch 'install'
$RuntimeFixture = Join-Path $Assets 'runtime'
$Wrapper = Join-Path $Scratch 'run-installer.ps1'

function New-TestBinary {
    param([string]$Path, [string]$Version)
    $previousVersion = $env:FOTON_INSTALL_TEST_VERSION
    $env:FOTON_INSTALL_TEST_VERSION = $Version
    try {
        & rustc (Join-Path $Repo 'dev\install-test-binary.rs') -o $Path
        if ($LASTEXITCODE -ne 0) { throw 'rustc could not build the installer fixture' }
    } finally {
        $env:FOTON_INSTALL_TEST_VERSION = $previousVersion
    }
}

function Set-TestBinary {
    param([string]$Source, [string]$Destination)
    if (Test-Path $Destination) { Remove-Item -Force $Destination }
    New-Item -ItemType HardLink -Path $Destination -Target $Source | Out-Null
}

function New-Runtime {
    param([string]$Path, [string]$Tag, [string]$Marker, [switch]$Installed)
    if (Test-Path $Path) { Remove-Item -Recurse -Force $Path }
    New-Item -ItemType Directory -Force -Path (Join-Path $Path 'lib'), (Join-Path $Path 'licenses') | Out-Null
    [System.IO.File]::WriteAllText((Join-Path $Path 'foton-plugin-api.jar'), "api $Marker`n")
    $libraries = @(
        'adventure-api-5.2.0.jar', 'adventure-key-5.2.0.jar',
        'adventure-text-logger-slf4j-5.2.0.jar', 'adventure-text-serializer-plain-5.2.0.jar',
        'annotations-26.1.0.jar', 'brigadier-1.3.10.jar',
        'error_prone_annotations-2.47.0.jar', 'failureaccess-1.0.3.jar',
        'gson-2.14.0.jar', 'guava-33.6.0-jre.jar', 'j2objc-annotations-3.1.jar',
        'joml-1.10.8.jar', 'jspecify-1.0.0.jar', 'kotlin-stdlib-1.8.20.jar',
        'kotlin-stdlib-common-1.8.20.jar', 'kotlin-stdlib-jdk7-1.8.20.jar',
        'kotlin-stdlib-jdk8-1.8.20.jar',
        'netty-buffer-4.2.15.Final.jar', 'netty-codec-base-4.2.15.Final.jar',
        'netty-common-4.2.15.Final.jar', 'netty-resolver-4.2.15.Final.jar',
        'netty-transport-4.2.15.Final.jar', 'slf4j-api-2.0.17.jar', 'snakeyaml-2.2.jar'
    )
    foreach ($name in $libraries) {
        [System.IO.File]::WriteAllText((Join-Path $Path "lib\$name"), "library $Marker $name`n")
    }
    $licenses = @(
        'ADVENTURE-MIT.txt',
        'APACHE-2.0.txt',
        'BRIGADIER-MIT.txt',
        'JOML-MIT.txt',
        'SLF4J-MIT.txt',
        'THIRD-PARTY-NOTICES.txt'
    )
    foreach ($license in $licenses) {
        [System.IO.File]::WriteAllText((Join-Path $Path "licenses\$license"), "license fixture $license`n")
    }
    $manifest = New-Object System.Collections.Generic.List[string]
    foreach ($relative in @('foton-plugin-api.jar') +
        @(Get-ChildItem (Join-Path $Path 'lib') -File | Sort-Object Name | ForEach-Object { "lib/$($_.Name)" }) +
        @($licenses | ForEach-Object { "licenses/$_" })) {
        $file = Join-Path $Path ($relative -replace '/', '\')
        $hash = (Get-FileHash -Algorithm SHA256 $file).Hash.ToLowerInvariant()
        $manifest.Add("$hash  $relative")
    }
    [System.IO.File]::WriteAllText((Join-Path $Path 'SHA256SUMS'), ($manifest -join "`n") + "`n")
    if ($Installed) {
        [System.IO.File]::WriteAllText((Join-Path $Path '.release-tag'), "$Tag`n")
    }
}

function New-ReleaseAssets {
    param([string]$Binary)
    $assetBinary = Join-Path $Assets 'foton-windows-x86_64.exe'
    Set-TestBinary $Binary $assetBinary
    $zip = Join-Path $Assets 'foton-plugin-runtime.zip'
    if (Test-Path $zip) { Remove-Item -Force $zip }
    Compress-Archive -Path (Join-Path $RuntimeFixture '*') -DestinationPath $zip
    $binaryHash = (Get-FileHash -Algorithm SHA256 $assetBinary).Hash.ToLowerInvariant()
    $zipHash = (Get-FileHash -Algorithm SHA256 $zip).Hash.ToLowerInvariant()
    [System.IO.File]::WriteAllText(
        (Join-Path $Assets 'SHA256SUMS'),
        "$binaryHash  foton-windows-x86_64.exe`n$zipHash  foton-plugin-runtime.zip`n"
    )
}

function Invoke-Installer {
    param([string]$Log, [switch]$UseIex, [switch]$FailMetadata, [switch]$Fresh)
    $environment = @{
        FOTON_INSTALL_TEST_ASSETS = $Assets
        FOTON_INSTALL_SCRIPT = (Join-Path $Repo 'site\static\install.ps1')
        FOTON_INSTALL_USE_IEX = $(if ($UseIex) { '1' } else { $null })
        FOTON_INSTALL_FAIL_METADATA = $(if ($FailMetadata) { '1' } else { $null })
        FOTON_INSTALL_FRESH = $(if ($Fresh) { '1' } else { $null })
        FOTON_INSTALL_NONINTERACTIVE = '1'
        FOTON_INSTALL_SENTINEL = (Join-Path $Scratch 'iex-sentinel')
    }
    $old = @{}
    foreach ($entry in $environment.GetEnumerator()) {
        $old[$entry.Key] = [Environment]::GetEnvironmentVariable($entry.Key)
        [Environment]::SetEnvironmentVariable($entry.Key, $entry.Value)
    }
    try {
        $savedErrorPreference = $ErrorActionPreference
        $ErrorActionPreference = 'Continue'
        try {
            $output = & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $Wrapper 2>&1
            $code = $LASTEXITCODE
        } finally {
            $ErrorActionPreference = $savedErrorPreference
        }
        $output | Set-Content -Encoding UTF8 $Log
        return $code
    } finally {
        foreach ($entry in $old.GetEnumerator()) {
            [Environment]::SetEnvironmentVariable($entry.Key, $entry.Value)
        }
    }
}

try {
    New-Item -ItemType Directory -Force -Path $Assets, $Install | Out-Null
    if (Select-String -Path (Join-Path $Repo 'site\static\install.ps1') -Pattern '\bexit\b' -Quiet) {
        throw 'PowerShell installer contains exit, which would close an irm/iex caller'
    }
    @'
$ErrorActionPreference = 'Stop'
function Invoke-RestMethod {
    param($Uri, [switch]$UseBasicParsing, $ErrorAction)
    if ($env:FOTON_INSTALL_FAIL_METADATA -eq '1') { throw 'simulated metadata failure' }
    [pscustomobject]@{
        tag_name = 'v9.8.7'
        assets = @(
            [pscustomobject]@{ name = 'foton-windows-x86_64.exe'; browser_download_url = 'fixture://foton-windows-x86_64.exe' },
            [pscustomobject]@{ name = 'foton-plugin-runtime.zip'; browser_download_url = 'fixture://foton-plugin-runtime.zip' },
            [pscustomobject]@{ name = 'SHA256SUMS'; browser_download_url = 'fixture://SHA256SUMS' }
        )
    }
}
function Invoke-WebRequest {
    param($Uri, $OutFile, [switch]$UseBasicParsing, $ErrorAction)
    $source = Join-Path $env:FOTON_INSTALL_TEST_ASSETS ([IO.Path]::GetFileName([string]$Uri))
    if ([IO.Path]::GetExtension($source) -eq '.exe') {
        New-Item -ItemType HardLink -Path $OutFile -Target $source | Out-Null
    } else {
        Copy-Item -Force $source $OutFile
    }
}
function Move-Item {
    param($Path, $Destination, [switch]$Force)
    Microsoft.PowerShell.Management\Move-Item -Path $Path -Destination $Destination -Force:$Force
}
function Copy-Item {
    param($Path, $Destination, [switch]$Recurse, [switch]$Force)
    if ($env:FOTON_INSTALL_FAIL_RESTORE -eq '1' -and ([string]$Path) -match 'previous-binary$') {
        throw 'simulated restore failure'
    }
    Microsoft.PowerShell.Management\Copy-Item -Path $Path -Destination $Destination -Recurse:$Recurse -Force:$Force
}
try {
    $installerArguments = @{}
    if ($env:FOTON_INSTALL_FRESH -ne '1') { $installerArguments.Update = $true }
    if ($env:FOTON_INSTALL_USE_IEX -eq '1') {
        $installer = Get-Content -Raw $env:FOTON_INSTALL_SCRIPT
        $suffix = if ($env:FOTON_INSTALL_FRESH -eq '1') { '' } else { ' -Update' }
        Invoke-Expression ("& {`n" + $installer + "`n}" + $suffix)
        [System.IO.File]::WriteAllText($env:FOTON_INSTALL_SENTINEL, 'continued')
    } else {
        & $env:FOTON_INSTALL_SCRIPT @installerArguments
    }
    exit 0
} catch {
    Write-Error $_
    exit 1
}
'@ | Set-Content -Encoding UTF8 $Wrapper

    New-Item -ItemType Directory -Force -Path (Join-Path $Install 'config'), (Join-Path $Install 'plugins'), (Join-Path $Install 'saves'), (Join-Path $Install 'plugin-runtime') | Out-Null
    'keep config' | Set-Content (Join-Path $Install 'config\sentinel')
    'keep plugin' | Set-Content (Join-Path $Install 'plugins\sentinel')
    'keep save' | Set-Content (Join-Path $Install 'saves\sentinel')
    'incomplete' | Set-Content (Join-Path $Install 'plugin-runtime\foton-plugin-api.jar')

    $oldGood = Join-Path $Scratch 'old-good.exe'
    $newGood = Join-Path $Scratch 'new-good.exe'
    $newBad = Join-Path $Scratch 'new-bad.exe'
    New-TestBinary $oldGood '9.8.7'
    New-TestBinary $newGood '9.8.7'
    New-TestBinary $newBad '0.0.0'
    Set-TestBinary $oldGood (Join-Path $Install 'foton.exe')
    New-Runtime $RuntimeFixture 'v9.8.7' 'new'
    New-ReleaseAssets $newGood

    # A fresh install must ask before replacing either half of the pair. In a
    # non-interactive host the safe default declines and preserves all data.
    $collision = Join-Path $Scratch 'runtime-only-collision'
    New-Item -ItemType Directory -Force -Path (Join-Path $collision 'plugin-runtime'), (Join-Path $collision 'saves') | Out-Null
    'keep runtime' | Set-Content (Join-Path $collision 'plugin-runtime\sentinel')
    'keep save' | Set-Content (Join-Path $collision 'saves\sentinel')
    Push-Location $collision
    try { $collisionCode = Invoke-Installer (Join-Path $Scratch 'collision.log') -Fresh } finally { Pop-Location }
    if ($collisionCode -eq 0) { throw 'fresh installer replaced a runtime-only collision without consent' }
    if ((Get-Content -Raw (Join-Path $collision 'plugin-runtime\sentinel')).Trim() -ne 'keep runtime') { throw 'runtime-only collision was modified' }
    if ((Get-Content -Raw (Join-Path $collision 'saves\sentinel')).Trim() -ne 'keep save') { throw 'runtime-only collision removed saves' }

    Push-Location $Install
    try { $repairCode = Invoke-Installer (Join-Path $Scratch 'repair.log') } finally { Pop-Location }
    if ($repairCode -ne 0) { throw "repair scenario failed with $repairCode" }
    if ((Get-Content -Raw (Join-Path $Install 'plugin-runtime\lib\adventure-api-5.2.0.jar')).Trim() -ne 'library new adventure-api-5.2.0.jar') { throw 'runtime was not repaired' }
    if ((Get-Content -Raw (Join-Path $Install 'plugin-runtime\lib\netty-codec-base-4.2.15.Final.jar')).Trim() -ne 'library new netty-codec-base-4.2.15.Final.jar') { throw 'Netty runtime was not repaired' }
    foreach ($sentinel in 'config\sentinel', 'plugins\sentinel', 'saves\sentinel') {
        if (-not (Test-Path (Join-Path $Install $sentinel))) { throw "$sentinel was removed" }
    }

    # Junction/reparse control paths must never redirect recovery or cleanup
    # outside the installation directory.
    $outside = Join-Path $Scratch 'outside-control-target'
    New-Item -ItemType Directory $outside | Out-Null
    'outside stays' | Set-Content (Join-Path $outside 'sentinel')
    $transactionJunction = Join-Path $Install '.foton-install-transaction'
    New-Item -ItemType Junction -Path $transactionJunction -Target $outside | Out-Null
    Push-Location $Install
    try { $junctionCode = Invoke-Installer (Join-Path $Scratch 'journal-junction.log') } finally { Pop-Location }
    if ($junctionCode -eq 0) { throw 'installer accepted a junction transaction journal' }
    if ((Get-Content -Raw (Join-Path $outside 'sentinel')).Trim() -ne 'outside stays') { throw 'journal junction modified outside data' }
    [System.IO.Directory]::Delete($transactionJunction)

    $lockJunction = Join-Path $Install '.foton-install.lock'
    New-Item -ItemType Junction -Path $lockJunction -Target $outside | Out-Null
    Push-Location $Install
    try { $lockJunctionCode = Invoke-Installer (Join-Path $Scratch 'lock-junction.log') } finally { Pop-Location }
    if ($lockJunctionCode -eq 0) { throw 'installer accepted a junction lock path' }
    if ((Get-Content -Raw (Join-Path $outside 'sentinel')).Trim() -ne 'outside stays') { throw 'lock junction modified outside data' }
    [System.IO.Directory]::Delete($lockJunction)

    New-Item -ItemType Directory $transactionJunction | Out-Null
    'not-an-owner' | Set-Content (Join-Path $transactionJunction 'owner')
    Push-Location $Install
    try { $ownerCode = Invoke-Installer (Join-Path $Scratch 'journal-owner.log') } finally { Pop-Location }
    if ($ownerCode -eq 0) { throw 'installer accepted an invalid journal owner marker' }
    if ((Get-Content -Raw (Join-Path $Scratch 'journal-owner.log')) -notmatch 'invalid ownership marker') { throw 'invalid owner rejection was not explained' }
    Remove-Item -Recurse -Force $transactionJunction

    # A truncated internal manifest is rejected even when its archive has a
    # freshly regenerated outer checksum.
    Set-TestBinary $oldGood (Join-Path $Install 'foton.exe')
    New-Runtime (Join-Path $Install 'plugin-runtime') 'v9.8.6' 'old' -Installed
    New-Runtime $RuntimeFixture 'v9.8.7' 'truncated'
    $manifestPath = Join-Path $RuntimeFixture 'SHA256SUMS'
    @(Get-Content $manifestPath | Select-Object -SkipLast 1) | Set-Content -Encoding ASCII $manifestPath
    New-ReleaseAssets $newGood
    Push-Location $Install
    try { $truncatedCode = Invoke-Installer (Join-Path $Scratch 'truncated.log') } finally { Pop-Location }
    if ($truncatedCode -eq 0) { throw 'installer accepted a truncated runtime manifest' }
    if ((Get-Content -Raw (Join-Path $Install 'plugin-runtime\lib\adventure-api-5.2.0.jar')).Trim() -ne 'library old adventure-api-5.2.0.jar') { throw 'truncated runtime changed the installed pair' }

    # A manifest that honestly describes only twenty-three jars still cannot redefine
    # the supported runtime set.
    New-Runtime $RuntimeFixture 'v9.8.7' 'shortened'
    Remove-Item (Join-Path $RuntimeFixture 'lib\netty-codec-base-4.2.15.Final.jar')
    $manifestPath = Join-Path $RuntimeFixture 'SHA256SUMS'
    @(Get-Content $manifestPath | Where-Object { $_ -notmatch 'lib/netty-codec-base-4\.2\.15\.Final\.jar$' }) | Set-Content -Encoding ASCII $manifestPath
    New-ReleaseAssets $newGood
    Push-Location $Install
    try { $shortenedCode = Invoke-Installer (Join-Path $Scratch 'shortened.log') } finally { Pop-Location }
    if ($shortenedCode -eq 0) { throw 'installer accepted only twenty-three dependency jars' }
    if ((Get-Content -Raw (Join-Path $Install 'plugin-runtime\lib\adventure-api-5.2.0.jar')).Trim() -ne 'library old adventure-api-5.2.0.jar') { throw 'shortened runtime changed the installed pair' }

    # Exact license names are mandatory even when the shortened manifest is
    # internally consistent.
    New-Runtime $RuntimeFixture 'v9.8.7' 'missing-license'
    Remove-Item (Join-Path $RuntimeFixture 'licenses\SLF4J-MIT.txt')
    $manifestPath = Join-Path $RuntimeFixture 'SHA256SUMS'
    @(Get-Content $manifestPath | Where-Object { $_ -notmatch 'licenses/SLF4J-MIT\.txt$' }) | Set-Content -Encoding ASCII $manifestPath
    New-ReleaseAssets $newGood
    Push-Location $Install
    try { $licenseCode = Invoke-Installer (Join-Path $Scratch 'missing-license.log') } finally { Pop-Location }
    if ($licenseCode -eq 0) { throw 'installer accepted a missing required license' }

    # A directory named like a jar must never reach the JVM classpath.
    New-Runtime $RuntimeFixture 'v9.8.7' 'special-jar'
    New-Item -ItemType Directory (Join-Path $RuntimeFixture 'lib\directory.jar') | Out-Null
    New-ReleaseAssets $newGood
    Push-Location $Install
    try { $specialCode = Invoke-Installer (Join-Path $Scratch 'special-jar.log') } finally { Pop-Location }
    if ($specialCode -eq 0) { throw 'installer accepted a special entry named as a jar' }

    # Nineteen jars is not enough: every name must match the runtime compiled
    # into Foton, or a same-version repair can commit an unusable bundle.
    New-Runtime $RuntimeFixture 'v9.8.7' 'renamed-jar'
    Move-Item (Join-Path $RuntimeFixture 'lib\failureaccess-1.0.3.jar') (Join-Path $RuntimeFixture 'lib\unexpected-1.0.jar')
    $manifest = New-Object System.Collections.Generic.List[string]
    foreach ($relative in @('foton-plugin-api.jar') +
        @(Get-ChildItem (Join-Path $RuntimeFixture 'lib') -File | Sort-Object Name | ForEach-Object { "lib/$($_.Name)" }) +
        @(Get-ChildItem (Join-Path $RuntimeFixture 'licenses') -File | Sort-Object Name | ForEach-Object { "licenses/$($_.Name)" })) {
        $file = Join-Path $RuntimeFixture ($relative -replace '/', '\')
        $hash = (Get-FileHash -Algorithm SHA256 $file).Hash.ToLowerInvariant()
        $manifest.Add("$hash  $relative")
    }
    [System.IO.File]::WriteAllText((Join-Path $RuntimeFixture 'SHA256SUMS'), ($manifest -join "`n") + "`n")
    New-ReleaseAssets $newGood
    Push-Location $Install
    try { $renamedCode = Invoke-Installer (Join-Path $Scratch 'renamed-jar.log') } finally { Pop-Location }
    if ($renamedCode -eq 0) { throw 'installer accepted an unexpected runtime jar name' }

    # Offer a binary that reports the wrong version. The transaction must put
    # the older pair back.
    New-Runtime $RuntimeFixture 'v9.8.7' 'new'
    New-ReleaseAssets $newBad
    Push-Location $Install
    try { $rollbackCode = Invoke-Installer (Join-Path $Scratch 'rollback.log') } finally { Pop-Location }
    if ($rollbackCode -eq 0) { throw 'installer accepted a binary with the wrong version' }
    $restoredVersion = & (Join-Path $Install 'foton.exe') --version
    if ($restoredVersion -ne 'foton 9.8.7') { throw "old binary was not restored: $restoredVersion" }
    if ((Get-Content -Raw (Join-Path $Install 'plugin-runtime\lib\adventure-api-5.2.0.jar')).Trim() -ne 'library old adventure-api-5.2.0.jar') { throw 'old runtime was not restored' }
    if ((Get-Content -Raw (Join-Path $Scratch 'rollback.log')) -notmatch 'previous installation was restored') { throw 'rollback result was not reported honestly' }

    [Environment]::SetEnvironmentVariable('FOTON_INSTALL_FAIL_RESTORE', '1')
    Push-Location $Install
    try { $failedRollbackCode = Invoke-Installer (Join-Path $Scratch 'rollback-failed.log') } finally { Pop-Location }
    [Environment]::SetEnvironmentVariable('FOTON_INSTALL_FAIL_RESTORE', $null)
    if ($failedRollbackCode -eq 0) { throw 'installer ignored a rollback failure' }
    $failedRollback = Get-Content -Raw (Join-Path $Scratch 'rollback-failed.log')
    if ($failedRollback -notmatch 'rollback was incomplete:') { throw 'rollback failure was not reported' }
    if ($failedRollback -notmatch 'binary backup') { throw 'rollback failure did not name its binary backup' }
    $transaction = Join-Path $Install '.foton-install-transaction'
    if (-not (Test-Path (Join-Path $transaction 'previous-binary'))) { throw 'failed rollback did not preserve its binary backup' }
    if (-not (Test-Path (Join-Path $transaction 'previous-runtime'))) { throw 'partial rollback did not preserve its runtime backup' }
    if ((Get-Content -Raw (Join-Path $Install 'plugin-runtime\lib\adventure-api-5.2.0.jar')).Trim() -ne 'library old adventure-api-5.2.0.jar') { throw 'partial rollback did not restore its runtime' }

    # A later process owns the destination lock, recovers the durable journal,
    # then reaches the deliberately failed metadata lookup.
    Push-Location $Install
    try { $recoveryCode = Invoke-Installer (Join-Path $Scratch 'recovery.log') -FailMetadata } finally { Pop-Location }
    if ($recoveryCode -eq 0) { throw 'metadata failure unexpectedly succeeded after recovery' }
    if ((Get-Content -Raw (Join-Path $Scratch 'recovery.log')) -notmatch 'Recovered an interrupted installation') { throw 'next launch did not report transaction recovery' }
    if (Test-Path $transaction) { throw 'recovered transaction journal remained' }
    if ((& (Join-Path $Install 'foton.exe') --version) -ne 'foton 9.8.7') { throw 'recovery did not restore the previous binary' }
    if ((Get-Content -Raw (Join-Path $Install 'plugin-runtime\lib\adventure-api-5.2.0.jar')).Trim() -ne 'library old adventure-api-5.2.0.jar') { throw 'recovery removed an already-restored runtime' }

    # FileShare.None makes the destination lock authoritative across processes.
    $lockPath = Join-Path $Install '.foton-install.lock'
    $heldLock = [System.IO.File]::Open($lockPath, [System.IO.FileMode]::OpenOrCreate, [System.IO.FileAccess]::ReadWrite, [System.IO.FileShare]::None)
    try {
        Push-Location $Install
        try { $concurrentCode = Invoke-Installer (Join-Path $Scratch 'concurrent.log') } finally { Pop-Location }
    } finally {
        $heldLock.Dispose()
        Remove-Item -Force $lockPath -ErrorAction SilentlyContinue
    }
    if ($concurrentCode -eq 0) { throw 'installer ignored an active destination lock' }
    if ((Get-Content -Raw (Join-Path $Scratch 'concurrent.log')) -notmatch 'another installer is already modifying') { throw 'lock failure was not explained' }

    # A successful no-op evaluated exactly like `irm | iex` must return to the
    # caller instead of closing its PowerShell session.
    Set-TestBinary $newGood (Join-Path $Install 'foton.exe')
    New-Runtime (Join-Path $Install 'plugin-runtime') 'v9.8.7' 'new' -Installed
    $iexSentinel = Join-Path $Scratch 'iex-sentinel'
    if (Test-Path $iexSentinel) { Remove-Item -Force $iexSentinel }
    Push-Location $Install
    try { $iexCode = Invoke-Installer (Join-Path $Scratch 'iex.log') -UseIex } finally { Pop-Location }
    if ($iexCode -ne 0) { throw "iex no-op failed with $iexCode" }
    if (-not (Test-Path $iexSentinel)) { throw 'installer exit closed the iex caller before its sentinel' }

    Write-Host 'PowerShell installer repair and rollback checked'
} catch {
    Get-ChildItem $Scratch -Filter '*.log' -ErrorAction SilentlyContinue | ForEach-Object {
        Write-Host "`n--- $($_.Name) ---"
        Get-Content $_.FullName
    }
    throw
} finally {
    if (Test-Path $Scratch) { Remove-Item -Recurse -Force $Scratch }
}
