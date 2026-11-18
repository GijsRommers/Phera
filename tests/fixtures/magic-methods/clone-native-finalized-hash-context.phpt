--TEST--
A finalized HashContext rejects cloning
--FILE--
<?php
$context = hash_init('sha256');
hash_final($context);
try {
    clone $context;
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}
--EXPECT--
Cannot clone a finalized HashContext
