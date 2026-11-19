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
Constant [ bool ENABLED ] { 1 }
Constant [ bool DISABLED ] {  }
Constant [ public bool ENABLED ] { 1 }
Constant [ protected bool DISABLED ] {  }
