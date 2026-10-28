# Records the #tour from a real (GPU-drawn) Chrome window on Windows.
#
# Starts Chrome with a throwaway profile and the DevTools protocol on, and
# saves every frame the page paints (Page.startScreencast) as frame-N.jpg,
# with its time in times.txt. art/capture_tour.sh runs this from WSL and
# turns the frames into a video. Run with PowerShell 7 (pwsh).
param(
  [string]$Url = "http://127.0.0.1:7700/#kenney.tour",
  [int]$Seconds = 35,
  [string]$Out = (Join-Path $env:TEMP "kube-city-capture"),
  [int]$Port = 9223
)
$ErrorActionPreference = "Stop"
$chrome = "C:\Program Files\Google\Chrome\Application\chrome.exe"
$profileDir = Join-Path $env:TEMP "kube-city-capture-profile"
Remove-Item -Recurse -Force $Out -ErrorAction SilentlyContinue
New-Item -ItemType Directory $Out | Out-Null

$browser = Start-Process $chrome -PassThru -ArgumentList @(
  "--user-data-dir=$profileDir", "--remote-debugging-port=$Port", "--no-first-run",
  "--no-default-browser-check", "--window-size=1280,800", "--app=$Url")

$page = $null
for ($i = 0; $i -lt 300 -and -not $page; $i++) {
  try { $page = (Invoke-RestMethod -NoProxy "http://127.0.0.1:$Port/json") | ForEach-Object { $_ } | Where-Object type -eq "page" | Select-Object -First 1 }
  catch { }
  if (-not $page) { Start-Sleep -Milliseconds 200 }
}
if (-not $page) { throw "Chrome did not open its DevTools port $Port" }

$none = [Threading.CancellationToken]::None
$ws = [Net.WebSockets.ClientWebSocket]::new()
$ws.ConnectAsync([Uri]$page.webSocketDebuggerUrl, $none).Wait()
$script:id = 0
function Send($method, $params) {
  $script:id++
  $json = @{ id = $script:id; method = $method; params = $params } | ConvertTo-Json -Compress -Depth 5
  $bytes = [Text.Encoding]::UTF8.GetBytes($json)
  $ws.SendAsync([ArraySegment[byte]]$bytes, "Text", $true, $none).Wait()
}
$buffer = [byte[]]::new(1MB)
function Receive {
  $message = [IO.MemoryStream]::new()
  do {
    $result = $ws.ReceiveAsync([ArraySegment[byte]]$buffer, $none).Result
    $message.Write($buffer, 0, $result.Count)
  } while (-not $result.EndOfMessage)
  [Text.Encoding]::UTF8.GetString($message.ToArray()) | ConvertFrom-Json
}

# Give the page a moment to load its sprites before recording.
Start-Sleep -Seconds 3
Send "Page.startScreencast" @{ format = "jpeg"; quality = 90; maxWidth = 1280; maxHeight = 800 }
$times = [Collections.Generic.List[string]]::new()
$clock = [Diagnostics.Stopwatch]::StartNew()
$n = 0
while ($clock.Elapsed.TotalSeconds -lt $Seconds) {
  $msg = Receive
  if ($msg.method -ne "Page.screencastFrame") { continue }
  Send "Page.screencastFrameAck" @{ sessionId = $msg.params.sessionId }
  [IO.File]::WriteAllBytes((Join-Path $Out "frame-$n.jpg"), [Convert]::FromBase64String($msg.params.data))
  $times.Add("$n $($msg.params.metadata.timestamp)")
  $n++
}
Send "Page.stopScreencast" @{}
[IO.File]::WriteAllLines((Join-Path $Out "times.txt"), $times)
Invoke-RestMethod -NoProxy "http://127.0.0.1:$Port/json/close/$($page.id)" | Out-Null
Start-Sleep -Seconds 1
if (-not $browser.HasExited) { Stop-Process -Id $browser.Id -ErrorAction SilentlyContinue }
Write-Output "$n frames in $Out"
