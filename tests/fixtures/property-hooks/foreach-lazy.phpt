--TEST--
Foreach calls each getter in declaration order as the body advances
--FILE--
<?php
class C {
    public int $a { get { echo "get a\n"; return 1; } }
    public int $hidden { set {} }
    public int $b { get { echo "get b\n"; return 2; } }
}
foreach (new C as $name => $value) {
    echo "$name $value\n";
    if ($value === 2) { break; }
}
--EXPECT--
get a
a 1
get b
b 2
