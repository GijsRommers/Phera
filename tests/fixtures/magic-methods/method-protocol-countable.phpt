--TEST--
Countable calls its declared method and ordinary missing calls reach __call
--FILE--
<?php
class MagicCount implements Countable {
    public function count(): int { echo "count\n"; return 2; }
    public function __call(string $name, array $args): mixed { echo "magic $name\n"; return 1; }
}
$value = new MagicCount;
var_dump(count($value), $value->missing());
--EXPECT--
count
magic missing
int(2)
int(1)
