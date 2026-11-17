<?php
$h = hash_init('sha256');
hash_update($h, 'hello');
echo hash_final(hash_copy($h)), "\n";
foreach (['Mt19937', 'PcgOneseq128XslRr64', 'Xoshiro256StarStar'] as $name) {
    $class = 'Random\\Engine\\' . $name;
    $engine = new $class(42);
    $randomizer = new Random\Randomizer($engine);
    echo bin2hex($engine->generate()), ' ', $randomizer->nextInt(), ' ',
         $randomizer->getInt(-100, 100), ' ', $randomizer->nextFloat(), ' ',
         bin2hex($randomizer->getBytes(7)), "\n";
}
$p = password_hash('password', PASSWORD_BCRYPT, ['cost' => 4]);
var_dump(password_verify('password', $p));
var_dump(password_get_info($p));
mt_srand(42);
echo mt_rand(), ' ', rand(1, 6), "\n";
