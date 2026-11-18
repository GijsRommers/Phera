--TEST--
Backing access is restricted to the hook receiver and the current hook frame
--FILE--
<?php
class C {
    public ?C $next = null;
    public int $n = 2 {
        get {
            echo "get\n";
            if ($this->next) {
                return $this->next->throughMethod();
            }
            return $this->n;
        }
    }
    public function throughMethod() { return $this->n; }
}
$first = new C;
$first->next = new C;
echo $first->n, "\n";
--EXPECT--
get
get
2
