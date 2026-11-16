--TEST--
Class conformance: namespace-absolute
--FILE--
<?php namespace App;class X {static function y(){return 7;}} echo \App\X::y();
--EXPECT--
7
