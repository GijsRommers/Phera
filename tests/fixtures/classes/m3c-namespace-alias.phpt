--TEST--
Class conformance: namespace-alias
--FILE--
<?php namespace App {class X {static function y(){return 7;}}} namespace Client {use App\X as Model; echo Model::y();}
--EXPECT--
7
