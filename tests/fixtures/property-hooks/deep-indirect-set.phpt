--TEST--
Writing an offset suspends a by-reference getter at 50000 levels
--FILE--
<?php
class Chain {
    public static int $depth = 0;
    private array $storage = [];
    public array $value {
        &get {
            if (++self::$depth === 50000) {
                throw new Error('Reached ' . self::$depth . ' getter frames');
            }
            $next = new Chain;
            $next->value[] = 1;
            return $this->storage;
        }
    }
}
$first = new Chain;
try {
    $first->value[] = 1;
} catch (Error $error) {
    echo $error->getMessage(), "\n";
}
--EXPECT--
Reached 50000 getter frames
