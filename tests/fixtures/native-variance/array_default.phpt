--TEST--
Built-in method compatibility displays its array default
--FILE--
<?php
class D extends PDO { public function prepare(int $query, array $options = []): PDOStatement|false {} }

--EXPECTF--

Fatal error: Declaration of D::prepare(int $query, array $options = []): PDOStatement|false must be compatible with PDO::prepare(string $query, array $options = []): PDOStatement|false in %s on line 2
