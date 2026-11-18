--TEST--
Countable can dispatch count on a backed enum case
--FILE--
<?php
enum Bundle: int implements Countable {
    case Triple = 3;
    public function count(): int { return $this->value; }
}
var_dump(count(Bundle::Triple));
--EXPECT--
int(3)
