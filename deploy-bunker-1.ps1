$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
& "$ScriptDir/scripts/deploy-bunker-1.ps1" @args
