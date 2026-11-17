--TEST--
Native classes: constant-class-string
--FILE--
<?php
$class="PDO";
 var_dump($class::FETCH_ASSOC);

--EXPECT--
int(2)
