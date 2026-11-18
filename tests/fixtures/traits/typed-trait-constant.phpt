--TEST--
traits: typed trait constant
--FILE--
<?php
trait T{public const string VALUE="yes";} class C{use T;} echo C::VALUE;
--EXPECT--
yes
