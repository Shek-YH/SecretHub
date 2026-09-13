$ErrorActionPreference = 'Stop'
$scanRoots = @('apps', 'crates', 'tests')
$files = foreach ($root in $scanRoots) { if (Test-Path $root) { rg --files $root } }
$filesToScan = $files | Where-Object { $_ -notmatch 'security_scan\.ps1$' }
$sentinel = $filesToScan | ForEach-Object { rg -a -n 'TEST_SECRET_123456|BEGIN (RSA|OPENSSH|EC) PRIVATE KEY' $_ 2>$null }
if ($sentinel) { $sentinel; throw 'sensitive sentinel found in source/test files' }
$tracked = git ls-files -- '.env' '*.env' '*.secrethub-backup'
if ($tracked) { $tracked; throw 'secret-bearing export is tracked by Git' }
Write-Output 'SecretHub security scan: clean'
