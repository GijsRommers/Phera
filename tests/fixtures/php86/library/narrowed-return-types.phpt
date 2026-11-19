--TEST--
Reflection observes the return types of filter_var_array and header_register_callback
--FILE--
<?php
foreach (['filter_var_array', 'header_register_callback'] as $name) {
    echo (new ReflectionFunction($name))->getReturnType(), PHP_EOL;
}
--EXPECT--
array|false
true
