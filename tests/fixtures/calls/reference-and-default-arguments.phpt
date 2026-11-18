--TEST--
By-reference and default arguments inside a function
--FILE--
<?php
function bump(&$value, $by = 1) { $value += $by; return func_num_args(); }
function typed(int &$value) { $value++; }
function fill(&$missing) { $missing = 'made'; }
function scale(float $x, int $times = 2, ?string $unit = null): string { return ($x * $times) . ($unit ?? ''); }
function run() {
    $count = 0;
    for ($i = 0; $i < 3; $i++) { echo bump($count), bump($count, 10), " "; }
    echo $count, "\n";
    $alias = &$count; bump($count); echo $alias, "\n";
    $text = '5'; typed($text); var_dump($text);
    fill($fresh); echo $fresh, "\n";
    echo scale(1.5), ' ', scale(1.5, 3), ' ', scale(2, 1, 'm'), "\n";
    try { $s = 'x'; typed($s); } catch (TypeError $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
    $list = [3, 1, 2]; $copy = $list; bump($list[0]); echo implode(',', $list), ' ', implode(',', $copy), "\n";
}
run();
--EXPECTF--
12 12 12 33
34
int(6)
made
3 4.5 2m
TypeError: typed(): Argument #1 ($value) must be of type int, string given, called in %s on line 14
4,1,2 3,1,2
