--TEST--
instanceof, isset, empty and ?? as conditions next to their edge cases: self, parent and static, a class in a string or an object, magic and ArrayAccess, ?? whose set left side decides without evaluating the right side.
--FILE--
<?php
// instanceof, isset, empty and ?? as conditions next to their edge cases: self, parent and static, a class in a string or an object, magic and ArrayAccess, ?? whose set left side decides without evaluating the right side.
class Base {} class Kid extends Base {
    public function test($other) {
        $r = [];
        if ($other instanceof self) { $r[] = 'self'; }
        if ($other instanceof parent) { $r[] = 'parent'; }
        if ($other instanceof static) { $r[] = 'static'; }
        if (!($other instanceof Kid)) { $r[] = 'not kid'; }
        return implode(',', $r) ?: 'none';
    }
}
final class Magic implements ArrayAccess {
    public function __isset($n) { echo "[isset $n]"; return $n !== 'no'; }
    public function __get($n) { echo "[get $n]"; return $n === 'zero' ? 0 : 'v'; }
    public function offsetExists($o): bool { echo "[exists $o]"; return $o !== 'no'; }
    public function offsetGet($o): mixed { echo "[offset $o]"; return $o === 'zero' ? 0 : 'v'; }
    public function offsetSet($o, $v): void {}
    public function offsetUnset($o): void {}
}
function f() {
    $k = new Kid; $b = new Base; $m = new Magic; $cls = 'Base';
    echo $k->test($k), ' ', $k->test($b), ' ', $k->test(new stdClass), "\n";
    if ($undefined instanceof Base) { echo "yes\n"; } else { echo "no\n"; }
    if ($k instanceof $cls) { echo "dynamic yes\n"; }
    if ($k instanceof $b) { echo "object yes\n"; }
    foreach (['a', 'no', 'zero'] as $n) {
        if (isset($m->$n)) { echo " isset-prop $n\n"; } else { echo " not-isset-prop $n\n"; }
        if (empty($m->$n)) { echo " empty-prop $n\n"; } else { echo " full-prop $n\n"; }
        if (isset($m[$n])) { echo " isset-dim $n\n"; }
        if (!empty($m[$n])) { echo " full-dim $n\n"; }
    }
    $arr = ['x' => null, 'y' => 0, 'z' => 'v'];
    foreach (['x', 'y', 'z', 'w'] as $key) {
        if ($arr[$key] ?? side('fallback')) { echo "coalesce true $key\n"; } else { echo "coalesce false $key\n"; }
        if (isset($arr[$key], $arr['z'])) { echo "both $key\n"; }
        if (!isset($arr[$key], $arr['z'])) { echo "not both $key\n"; }
    }
    $i = 0; while (isset($arr['z']) && $i < 2) { $i++; } echo "while $i\n";
    do { $i--; } while (!empty($i)); echo "do $i\n";
    if ($k instanceof Base && !($b instanceof Kid) || isset($nope)) { echo "combined\n"; }
}
function side($s) { echo "[$s]"; return false; }
function coalesce() {
    $v = ['set' => 'yes', 'zero' => 0, 'null' => null];
    foreach (['set', 'zero', 'null', 'missing'] as $k) {
        $seen = [];
        if (!($v[$k] ?? side("r1 $k"))) { $seen[] = 'not'; }
        if (($v[$k] ?? side("r2 $k")) && true) { $seen[] = 'and'; }
        if (false || ($v[$k] ?? side("r3 $k"))) { $seen[] = 'or'; }
        if ($v[$k] ?? $v['null'] ?? side("r4 $k")) { $seen[] = 'chain'; }
        $n = 0; while (($v[$k] ?? false) && $n < 1) { $n++; } $seen[] = "while$n";
        echo " $k: ", implode(',', $seen), "\n";
    }
}
coalesce();
f();
--EXPECTF--
 set: and,or,chain,while1
 zero: not,while0
[r1 null][r2 null][r3 null][r4 null] null: not,while0
[r1 missing][r2 missing][r3 missing][r4 missing] missing: not,while0
self,parent,static parent,not kid not kid

Warning: Undefined variable $undefined in %s on line 24
no
dynamic yes
object yes
[isset a] isset-prop a
[isset a][get a] full-prop a
[exists a] isset-dim a
[exists a][offset a] full-dim a
[isset no] not-isset-prop no
[isset no] empty-prop no
[exists no][exists no][isset zero] isset-prop zero
[isset zero][get zero] empty-prop zero
[exists zero] isset-dim zero
[exists zero][offset zero][fallback]coalesce false x
not both x
coalesce false y
both y
coalesce true z
both z
[fallback]coalesce false w
not both w
while 2
do 0
combined
