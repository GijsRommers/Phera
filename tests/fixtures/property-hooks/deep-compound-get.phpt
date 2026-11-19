--TEST--
Hook frames resume a compound property update after deep calls
--FILE--
<?php
class Chain {
    public static int $depth = 0;
    public int $value {
        get {
            if (++self::$depth === 50000) {
                throw new Error('Reached ' . self::$depth . ' getter frames');
            }
            $next = new Chain;
            return $next->value += 1;
        }
        set {}
    }
}
try {
    var_dump((new Chain)->value);
} catch (Error $error) {
    echo $error->getMessage(), "\n";
}
--EXPECT--
Reached 50000 getter frames
