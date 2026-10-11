$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
& "$ScriptDir/scripts/deploy-bunker-2.ps1" @args
