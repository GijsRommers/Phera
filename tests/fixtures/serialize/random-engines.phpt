--TEST--
Native random engines preserve exact bytes and continue at the next draw
--FILE--
<?php
$engines = [new Random\Engine\Mt19937(42), new Random\Engine\PcgOneseq128XslRr64(42),
    new Random\Engine\Xoshiro256StarStar(42)];
foreach ($engines as $engine) {
    echo hash('sha256', serialize($engine)), "\n";
    $engine->generate();
    $blob = serialize($engine);
    echo hash('sha256', $blob), "\n";
    $restored = unserialize($blob);
    var_dump($engine->generate() === $restored->generate());
    var_dump(serialize($engine) === serialize($restored));
}
$r = new Random\Randomizer(new Random\Engine\PcgOneseq128XslRr64(42));
$copy = unserialize(serialize($r));
var_dump($r->getInt(1, 10) === $copy->getInt(1, 10));
?>
--EXPECT--
40a0f1c56da3c0d63fb4c905ad99de7b89282cbe9c00a086977b0399fe26fb37
3c367e24093c1029a443d572c5ea962b385fd1f46c89d486c7aea901cc0b058b
bool(true)
bool(true)
608f8ac3e01f2300c3188728feaff7490598d7644700f58a6c35f4985d459992
39d17c62ed6a1c5b85752439fb5ab061968b5e80d717b58c25af9da7adb239d5
bool(true)
bool(true)
feb25aef76e3865eae24e3120af7961b429393e85ddf522711697df0ba308e8a
69aeeb6d2ae402c3ea57d6fc52c33469ff74e273c0d7fa52df0a9df3eec73e5d
bool(true)
bool(true)
bool(true)
