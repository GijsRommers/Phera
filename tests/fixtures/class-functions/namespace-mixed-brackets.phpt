--TEST--
An unbracketed namespace cannot follow a bracketed namespace
--FILE--
<?php
namespace First {}
namespace Second;
--EXPECTF--
Fatal error: Cannot mix bracketed namespace declarations with unbracketed namespace declarations in %s on line 3
