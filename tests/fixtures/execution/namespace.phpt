--TEST--
namespace
--FILE--
<?php
namespace X;function f(){echo 42;}f();echo __NAMESPACE__;
--EXPECT--
42X