--TEST--
Exceptions: catch_basic
--FILE--
<?php
try { throw new Exception("x"); } catch (Exception $error) { echo $error->getMessage(); }
--EXPECTF--
x