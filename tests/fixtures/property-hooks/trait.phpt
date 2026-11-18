--TEST--
A trait hook is executed with access to the consuming class scope
--FILE--
<?php
trait Name {
    public string $name { get => $this->privateName; }
}
class Person {
    use Name;
    private string $privateName = 'Ada';
}
echo (new Person)->name, "\n";
--EXPECT--
Ada
