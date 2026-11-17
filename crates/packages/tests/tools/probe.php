<?php
// Run with plain PHP from an installed fixture; JSON is compared by compare.py.
$vendor = $argv[1];
$loader = require $vendor . '/autoload.php';
$normalize = static function ($path) {
    $root = getcwd() . '/';
    if (realpath($path) === getcwd()) { return '.'; }
    return str_replace($root, '', str_replace('\\', '/', realpath($path) ?: $path));
};
$result = array('classes' => array(), 'files' => array(), 'psr4' => array(), 'psr0' => array());
$map = $loader->getClassMap();
foreach ($map as $name => $path) {
    $result['classes'][$name] = $normalize($loader->findFile($name));
}
foreach (array('Phera\\DefinitelyMissing', 'NoSuchPheraClass',
    'Composer\\DefinitelyMissing') as $name) {
    $result['classes'][$name] = $loader->findFile($name);
}
ksort($result['classes']);
foreach ($loader->getPrefixesPsr4() as $prefix => $paths) {
    $result['psr4'][$prefix] = array_map($normalize, $paths);
}
foreach ($loader->getPrefixes() as $prefix => $paths) {
    $result['psr0'][$prefix] = array_map($normalize, $paths);
}
ksort($result['psr4']);
ksort($result['psr0']);
foreach ($GLOBALS['__composer_autoload_files'] ?? array() as $key => $loaded) {
    $result['files'][] = $key;
}
echo "\nPHERA_PROBE_JSON=" . json_encode($result, JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES);
