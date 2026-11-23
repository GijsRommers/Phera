--TEST--
Class exists loads a bundled Phera class (Phera's own expectation; real PHP has no Phera classes)
--FILE--
<?php
// A bundled Phera\ class is not loaded at start-up; the general lookup
// loads it from its source even with no autoloader registered.
var_dump(class_exists('Phera\Symfony\Runtime', false));
try { var_dump(class_exists('Phera\Symfony\Runtime')); } catch (Throwable $e) { echo 'caught: ', $e->getMessage(), "\n"; }
try { var_dump(class_exists('phera\symfony\runtime')); } catch (Throwable $e) { echo 'caught: ', $e->getMessage(), "\n"; }
try { var_dump(class_exists('\Phera\Symfony\Runtime')); } catch (Throwable $e) { echo 'caught: ', $e->getMessage(), "\n"; }
try { var_dump(class_exists('Phera\Symfony\Runtime', false)); } catch (Throwable $e) { echo 'caught: ', $e->getMessage(), "\n"; }
try { $r = new Phera\Symfony\Runtime([]); var_dump(get_class($r)); } catch (Throwable $e) { echo 'use caught: ', $e->getMessage(), "\n"; }
--EXPECT--
bool(false)
caught: Class "Symfony\Component\Runtime\SymfonyRuntime" not found
caught: Class "Symfony\Component\Runtime\SymfonyRuntime" not found
caught: Class "Symfony\Component\Runtime\SymfonyRuntime" not found
bool(false)
use caught: Class "Symfony\Component\Runtime\SymfonyRuntime" not found
