--TEST--
Class conformance: probe-get-class-self
--FILE--
<?php class D {function make(){return new self;}}echo get_class((new D)->make());
--EXPECT--
D
