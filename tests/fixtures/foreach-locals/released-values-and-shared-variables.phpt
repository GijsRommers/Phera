--TEST--
The loop variables of a by-value foreach written directly: what replaces their old value, references and globals in them, and the subject itself.
--FILE--
<?php
// The loop variables of a by-value foreach written directly: what replaces
// their old value, references and globals in them, and the subject itself.
final class Noisy { public function __construct(public string $name) {} public function __destruct() { echo "destruct {$this->name}\n"; } }
function release() {
    $v = new Noisy('old value');
    $k = new Noisy('old key');
    foreach (['a' => 1, 'b' => 2] as $k => $v) { echo "body $k $v\n"; }
    echo "after\n";
}
release();
function objects() {
    foreach ([new Noisy('first'), new Noisy('second')] as $v) { echo "body {$v->name}\n"; }
    echo "loop done\n";
}
objects();
$g = 'global before';
function viaGlobal() { global $g; foreach ([1, 2] as $g) {} }
viaGlobal(); var_dump($g);
function viaStatic() { static $s = 0; foreach ([3, 4] as $s) {} return $s; }
var_dump(viaStatic(), viaStatic());
$a = [1, 2, 3];
foreach ($a as $a) { echo "item $a\n"; }
var_dump($a);
$list = [1, 2, 3];
foreach ($list as $k => $v) { if ($k === 0) { $list[] = 4; unset($list[1]); } echo "$k=$v "; }
echo "\n", json_encode($list), "\n";
foreach ([1, 2, 3, 4] as $v) { if ($v === 3) break; echo "$v "; }
echo "after break $v\n";
final class Count3 implements Iterator {
    private int $i = 0;
    public function current(): mixed { echo "current\n"; return $this->i * 10; }
    public function key(): mixed { echo "key\n"; return "k{$this->i}"; }
    public function next(): void { echo "next\n"; $this->i++; }
    public function rewind(): void { echo "rewind\n"; $this->i = 0; }
    public function valid(): bool { echo "valid\n"; return $this->i < 2; }
}
foreach (new Count3 as $key => $value) { echo "$key => $value\n"; }
foreach ([[1, 2], [3]] as $row) { foreach ($row as $row) { echo "inner $row\n"; } echo "outer after inner: ", json_encode($row), "\n"; }
function undefinedAfter() { foreach ([] as $never) {} var_dump(isset($never)); }
undefinedAfter();
--EXPECT--
destruct old value
destruct old key
body a 1
body b 2
after
body first
body second
destruct first
loop done
destruct second
int(2)
int(4)
int(4)
item 1
item 2
item 3
int(3)
0=1 1=2 2=3 
{"0":1,"2":3,"3":4}
1 2 after break 3
rewind
valid
current
key
k0 => 0
next
valid
current
key
k1 => 10
next
valid
inner 1
inner 2
outer after inner: 2
inner 3
outer after inner: 3
bool(false)
