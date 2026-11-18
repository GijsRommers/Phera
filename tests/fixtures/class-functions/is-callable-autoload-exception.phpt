--TEST--
Is callable autoload exception
--FILE--
<?php
spl_autoload_register(function($name) { throw new Exception($name); }); try { is_callable("Known::run"); } catch (Exception $e) { echo $e->getMessage(); }
--EXPECT--
Known