--TEST--
Built-in method compatibility displays its negative default
--FILE--
<?php
class D extends RecursiveIteratorIterator { public function setMaxDepth(string $maxDepth = ''): void {} }

--EXPECTF--

Fatal error: Declaration of D::setMaxDepth(string $maxDepth = ''): void must be compatible with RecursiveIteratorIterator::setMaxDepth(int $maxDepth = -1): void in %s on line 2
