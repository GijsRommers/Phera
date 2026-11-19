--TEST--
Hook frames resume a property read into a pending argument after deep calls
--FILE--
<?php
function identity(int $value): int { return $value; }
class Chain {
    public static int $depth = 0;
    public int $value {
        get {
            if (++self::$depth === 50000) {
                throw new Error('Reached ' . self::$depth . ' getter frames');
            }
            return identity((new Chain)->value);
        }
    }
}
try {
    var_dump((new Chain)->value);
} catch (Error $error) {
    echo $error->getMessage(), "\n";
}
--EXPECT--
Reached 50000 getter frames
