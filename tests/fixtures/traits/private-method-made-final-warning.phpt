--TEST--
Making a private trait method final warns on the using class declaration
--FILE--
<?php
trait T { private function method() {} }
echo "before
";
class C {
    use T {
        method as final;
    }
}
echo "after
";
--EXPECTF--
before

Warning: Private methods cannot be final as they are never overridden by other classes in %s on line 5
after
