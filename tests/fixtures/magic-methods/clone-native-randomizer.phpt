--TEST--
Randomizer rejects cloning even if its engine is cloneable
--FILE--
<?php
try {
    clone new Random\Randomizer(new Random\Engine\Mt19937(123));
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}
--EXPECT--
Trying to clone an uncloneable object of class Random\Randomizer
