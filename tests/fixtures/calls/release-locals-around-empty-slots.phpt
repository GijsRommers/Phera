--TEST--
Returning releases locals in order and clears them before the window is reused
--FILE--
<?php
class Marker {
    public function __construct(private string $name) {}
    public function __destruct() { echo '[', $this->name, ']'; }
}
function releaseLocals(bool $fill): void {
    $first = new Marker('first');
    if ($fill) { $a = $b = $c = $d = 1; }
    $middle = new Marker('middle');
    if ($fill) { $e = $f = $g = $h = 2; }
    if (isset($h)) { echo 'filled '; }
    echo 'return ';
}
releaseLocals(true);
echo "\n";
releaseLocals(false);
echo "\n";
--EXPECT--
filled return [first][middle]
return [first][middle]
