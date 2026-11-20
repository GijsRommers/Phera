--TEST--
An alias created at runtime can satisfy a property contract
--FILE--
<?php
class Model {}
class_alias(Model::class, 'ModelName');
class ParentType {
    public Model $value;
}
class ChildType extends ParentType {
    public ModelName $value;
}
$obj = new ChildType();
$obj->value = new Model();
echo get_debug_type($obj->value), "\n";
--EXPECT--
Model
