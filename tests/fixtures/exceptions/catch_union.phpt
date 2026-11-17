--TEST--
Exceptions: catch_union
--FILE--
<?php
try { throw new TypeError("x"); } catch (Exception|TypeError $error) { echo get_class($error); }
--EXPECTF--
TypeError