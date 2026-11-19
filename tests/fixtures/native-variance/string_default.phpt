--TEST--
Built-in method compatibility displays its string default
--FILE--
<?php
class D extends SplFileInfo { public function getBasename(int $suffix = 0): string {} }

--EXPECTF--

Fatal error: Declaration of D::getBasename(int $suffix = 0): string must be compatible with SplFileInfo::getBasename(string $suffix = ""): string in %s on line 2
