--TEST--
long-chain-released-at-once
--FILE--
<?php
class Node { public static int $destroyed = 0; public $next; function __destruct() { self::$destroyed++; } }
$head = null;
for ($i = 0; $i < 50000; $i++) { $node = new Node; $node->next = $head; $head = $node; }
unset($node);
$head = null;
echo Node::$destroyed, "\n";
--EXPECT--
50000
