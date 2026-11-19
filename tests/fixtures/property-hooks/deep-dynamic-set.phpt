--TEST--
Dynamic writes suspend a setter at 50000 levels
--FILE--
<?php
class Chain {
    public int $value {
        set {
            if ($value === 50000) {
                throw new Error('Reached ' . $value . ' setter frames');
            }
            $next = new Chain;
            $name = 'value';
            $next->$name = $value + 1;
        }
    }
}
$first = new Chain;
try {
    $first->value = 1;
} catch (Error $error) {
    echo $error->getMessage(), "\n";
}
--EXPECT--
Reached 50000 setter frames
