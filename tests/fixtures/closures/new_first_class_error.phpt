--TEST--
new_first_class_error
--FILE--
<?php
class Foo {}
new Foo(...);
--EXPECTF--
Fatal error: Cannot create Closure for new expression in %s on line 3
