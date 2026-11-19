--TEST--
Isset, empty and coalesce assignment consult the getter
--FILE--
<?php
class C {
    public ?int $n = null {
        get { echo "get\n"; return $this->n; }
        set { echo "set\n"; $this->n = $value; }
    }
}
$c = new C;
var_dump(isset($c->n), empty($c->n));
var_dump($c->n ??= 4);
var_dump($c->n ??= 5);
--EXPECT--
get
get
bool(false)
bool(true)
get
set
int(4)
get
int(4)
