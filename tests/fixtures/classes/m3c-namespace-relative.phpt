--TEST--
Class conformance: namespace-relative
--FILE--
<?php namespace App;class X {static function y(){return 7;}} echo namespace\X::y();
--EXPECT--
7
