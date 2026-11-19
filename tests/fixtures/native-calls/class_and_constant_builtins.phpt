--TEST--
Direct defined and class introspection follow request declarations and called scope
--FILE--
<?php
class BaseScope {
    public static function called(): string { return get_called_class(); }
}
class DerivedScope extends BaseScope {}
var_dump(defined('NATIVE_CALLS_TEST'), defined('NULL'));
define('NATIVE_CALLS_TEST', 1);
var_dump(defined('NATIVE_CALLS_TEST'), defined('native_calls_test'));
var_dump(get_class(new DerivedScope()), DerivedScope::called());
--EXPECT--
bool(false)
bool(true)
bool(true)
bool(false)
string(12) "DerivedScope"
string(12) "DerivedScope"
