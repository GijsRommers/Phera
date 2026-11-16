--TEST--
autovivify
--FILE--
<?php
$a['x']['y'][]=42;echo $a['x']['y'][0];
--EXPECT--
42