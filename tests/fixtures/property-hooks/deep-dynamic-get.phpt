--TEST--
Hook frames resume a dynamic property read after deep calls
--FILE--
<?php
class Chain {
    public static int $depth = 0;
    public int $value {
        get {
            if (++self::$depth === 50000) {
                throw new Error('Reached ' . self::$depth . ' getter frames');
            }
            $name = 'value';
            return (new Chain)->$name;
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
