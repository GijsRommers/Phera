--TEST--
anonymous-classes: namespaced parent prefix
--FILE--
<?php
namespace App; class ParentType {} $x = new class extends ParentType {}; echo strstr(get_class($x), chr(0), true);
--EXPECT--
App\ParentType@anonymous
