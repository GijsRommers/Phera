--TEST--
traits: class is not a trait
--FILE--
<?php
class T{} try{class C{use T;}}catch(Error $e){echo $e->getMessage();}
--EXPECT--
C cannot use T - it is not a trait
