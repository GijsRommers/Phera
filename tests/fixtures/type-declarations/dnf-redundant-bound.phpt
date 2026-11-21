--TEST--
A DNF alternative cannot repeat a less restrictive bound
--FILE--
<?php
namespace Demo;
use External\Contract as Bound;
function invalid((Bound&Other)|Bound $value) {}
--EXPECTF--
Fatal error: Type External\Contract&Demo\Other is redundant as it is more restrictive than type External\Contract in %s on line 4
