--TEST--
traits: private final constructor cannot be redeclared
--FILE--
<?php
trait T { private final function __construct() {} }
class C { use T; }
class D extends C { public function __construct() {} }
--EXPECTF--
Fatal error: Cannot override final method C::__construct() in /private%s on line 4
