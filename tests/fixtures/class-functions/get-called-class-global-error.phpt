--TEST--
Get called class global error
--FILE--
<?php
try { get_called_class(); } catch (Error $e) { echo $e->getMessage(); }
--EXPECT--
get_called_class() must be called from within a class