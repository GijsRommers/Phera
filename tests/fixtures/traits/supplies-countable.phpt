--TEST--
A trait supplies the method for Countable
--FILE--
<?php
trait CountMethod {
    public function count(): int { return $this->size; }
}
class Items implements Countable {
    use CountMethod;
    private int $size = 4;
}
var_dump(count(new Items));
--EXPECT--
int(4)
