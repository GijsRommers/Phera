--TEST--
Reflection displays the value of a boolean constant
--FILE--
<?php
const ENABLED = true;
const DISABLED = false;
class Settings {
    public const ENABLED = true;
    protected const DISABLED = false;
}
echo new ReflectionConstant('ENABLED');
echo new ReflectionConstant('DISABLED');
echo new ReflectionClassConstant(Settings::class, 'ENABLED');
echo new ReflectionClassConstant(Settings::class, 'DISABLED');
--EXPECT--
Constant [ bool ENABLED ] { true }
Constant [ bool DISABLED ] { false }
Constant [ public bool ENABLED ] { true }
Constant [ protected bool DISABLED ] { false }
