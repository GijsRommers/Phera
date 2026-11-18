--TEST--
traits: incompatible static and instance property
--FILE--
<?php
trait T { static $p = true; } class C { use T; public $p = true; }
--EXPECTF--
Fatal error: C and T define the same property ($p) in the composition of C. However, the definition differs and is considered incompatible. Class was composed in /private%s on line 2
