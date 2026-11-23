--TEST--
Method exists members and visibility
--FILE--
<?php
class Other { private function privMethod(): void {} }
class Base {
    public int $pub = 1;
    protected int $prot = 2;
    private int $priv = 3;
    public static string $stat = 's';
    public function pubMethod(): void {}
    protected function protMethod(): void {}
    private function privMethod(): void {}
    public static function statMethod(): void {}
}
class Child extends Base {}
$base = new Base;
$closure = function (): void {};
var_dump(method_exists($base, 'pubMethod'));
var_dump(method_exists($base, 'PUBMETHOD'));
var_dump(method_exists($base, 'protMethod'));
var_dump(method_exists($base, 'privMethod'));
var_dump(method_exists($base, 'statMethod'));
var_dump(method_exists($base, 'nope'));
var_dump(method_exists('Base', 'privMethod'));
var_dump(method_exists('Child', 'privMethod'));
var_dump(method_exists('Other', 'privMethod'));
var_dump(method_exists($closure, '__invoke'));
var_dump(method_exists('Closure', '__invoke'));
var_dump(method_exists('Nope', 'm'));
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(false)
bool(true)
bool(false)
bool(true)
bool(true)
bool(true)
bool(false)
