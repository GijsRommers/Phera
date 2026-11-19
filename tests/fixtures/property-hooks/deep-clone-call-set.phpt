--TEST--
Callable clone suspends a setter at 50000 levels
--FILE--
<?php
class Chain {
    public int $value {
        set {
            if ($value === 50000) {
                throw new Error('Reached ' . $value . ' setter frames');
            }
            $clone = 'clone';
            $clone($this, ['value' => $value + 1]);
        }
    }
}
try {
    $clone = 'clone';
    $clone(new Chain, ['value' => 1]);
} catch (Error $error) {
    echo $error->getMessage(), "\n";
}
--EXPECT--
Reached 50000 setter frames
