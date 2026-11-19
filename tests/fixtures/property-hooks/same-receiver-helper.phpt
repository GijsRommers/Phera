--TEST--
A helper called by a hook resolves the property through its getter again
--FILE--
<?php
class C {
    private bool $nested = false;
    public int $n = 4 {
        get {
            echo "get\n";
            if ($this->nested) {
                return $this->n;
            }
            $this->nested = true;
            return $this->helper() + 1;
        }
    }
    private function helper() { return $this->n; }
}
echo (new C)->n, "\n";
--EXPECT--
get
get
5
