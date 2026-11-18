--TEST--
Interface extends loads interface
--FILE--
<?php
spl_autoload_register(function($name) { echo $name, "\n"; interface ParentI {} }); interface ChildI extends ParentI {} var_dump(interface_exists("ChildI", false));
--EXPECT--
ParentI
bool(true)
