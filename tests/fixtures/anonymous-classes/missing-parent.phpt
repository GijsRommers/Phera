--TEST--
anonymous-classes: missing parent
--FILE--
<?php
try { new class extends Missing {}; } catch (Error $e) { echo $e->getMessage(); }
--EXPECT--
Class "Missing" not found
