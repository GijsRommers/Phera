--TEST--
abandoned-generator-locals
--FILE--
<?php
// Unsetting a suspended generator closes it: its locals are destructed
// at the unset, before the next statement.
class T { function __construct(public $n) {} function __destruct() { echo "-{$this->n}\n"; } }
function gen() { $l = new T('generator local'); try { yield 1; yield 2; } finally { echo "finally\n"; } }
$g = gen();
$g->current();
unset($g);
echo "after unset\n";
$h = (function () { $l = new T('closure generator local'); yield 1; })();
$h->current();
$h = null;
echo "after null\n";
--EXPECT--
finally
-generator local
after unset
-closure generator local
after null
