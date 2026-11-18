--TEST--
traits: alias ambiguous
--FILE--
<?php
trait T{function m(){}} trait U{function m(){}} class C{use T,U {T::m insteadof U; m as n;}}
--EXPECTF--
Fatal error: An alias was defined for method m(), which exists in both T and U. Use T::m or U::m to resolve the ambiguity in %s on line 2
