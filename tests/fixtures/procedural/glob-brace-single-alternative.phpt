--TEST--
glob-brace-single-alternative
--FILE--
<?php
// A brace group with one alternative is expanded too: Symfony imports
// "{config}/{packages}/*.{php,yaml}". Only a pattern that is exactly "{}"
// stays literal.
$root = sys_get_temp_dir() . '/glob-brace-' . getmypid();
mkdir("$root/config/packages", 0777, true);
mkdir("$root/{}");
touch("$root/config/packages/a.yaml");
touch("$root/config/packages/b.php");
chdir($root);
foreach (['{config}/{packages}/*.{php,yaml}', '{config}', '{{config}}', '{config}{,}', '{config,}/packages/a.yaml', '{}', 'x{config}'] as $pattern) {
    echo $pattern, ' => ', json_encode(glob($pattern, GLOB_BRACE)), "\n";
}
unlink("$root/config/packages/a.yaml");
unlink("$root/config/packages/b.php");
rmdir("$root/config/packages");
rmdir("$root/config");
rmdir("$root/{}");
rmdir($root);
--EXPECT--
{config}/{packages}/*.{php,yaml} => ["config\/packages\/b.php","config\/packages\/a.yaml"]
{config} => ["config"]
{{config}} => ["config"]
{config}{,} => ["config","config"]
{config,}/packages/a.yaml => ["config\/packages\/a.yaml"]
{} => ["{}"]
x{config} => []
