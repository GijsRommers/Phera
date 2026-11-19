--TEST--
Accessible hooks are preferred to magic property methods
--FILE--
<?php
class C {
    public $x = 1 {
        get { echo "hook get\n"; return $this->x; }
        set { echo "hook set\n"; $this->x = $value; }
    }
    public function __get($key) { echo "magic get $key\n"; }
    public function __set($key, $value) { echo "magic set $key\n"; }
}
$c = new C;
$c->x = 7;
echo $c->x, "\n";
$c->y = 8;
$c->y;
--EXPECT--
hook set
hook get
7
magic set y
magic get y
