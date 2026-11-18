--TEST--
anonymous-classes: namespace import prefix
--FILE--
<?php
namespace App; class ParentType {} use App\ParentType as Base; $x = new class extends Base {}; echo strstr(get_class($x), chr(0), true);
--EXPECT--
App\ParentType@anonymous
