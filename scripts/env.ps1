$bin = "C:\Users\msrov\Tools\mingw64\mingw64\bin"
if ($env:PATH -notlike "*$bin*") { $env:PATH = "$bin;$env:PATH" }
$env:CARGO_TERM_COLOR = "never"
