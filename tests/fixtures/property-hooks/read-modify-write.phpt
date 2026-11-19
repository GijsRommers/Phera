--TEST--
Compound assignment and increment call get before set
--FILE--
<?php
class C {
    public int $n = 1 {
        get { echo "get\n"; return $this->n; }
        set { echo "set $value\n"; $this->n = $value; }
    }
}
$c = new C;
var_dump($c->n += 3);
var_dump($c->n++);
var_dump($c->n);
--EXPECT--
get
set 4
int(4)
get
set 5
int(4)
get
int(5)
