--TEST--
File-scope symbols reach each reader before and after compiling its include
--FILE--
<?php
class GlobalScopeReader {
    static function read() {
        global $probe;
        return $probe;
    }
}
function writeFromFunction(string $file): void {
    $local = 'still local';
    require $file;
    if ($local !== 'still local' || isset($includeOnly) === false) {
        throw new Exception('the function include lost its local scope');
    }
}
function readInFunction(string $file) {
    return require $file;
}
$byClosure = static function () {
    global $probe;
    return $probe;
};
$readers = <<<'READER'
<?php
if ($reading) {
    $dynamicName = implode(['pr', 'obe']);
    $actual = [
        $probe, $GLOBALS['probe'], $$dynamicName,
        compact('probe')['probe'], get_defined_vars()['probe'], isset($probe),
        $byClosure(), GlobalScopeReader::read(), readInFunction($readLocal),
    ];
    require $readGlobal;
}
READER;
$writeGlobal = tempnam(sys_get_temp_dir(), 'scope');
$writeLocal = tempnam(sys_get_temp_dir(), 'scope');
$readGlobal = tempnam(sys_get_temp_dir(), 'scope');
$readLocal = tempnam(sys_get_temp_dir(), 'scope');
file_put_contents($writeGlobal, '<?php $probe = "value";');
file_put_contents($writeLocal, '<?php global $probe; $probe = "value"; $includeOnly = 1;');
file_put_contents($readLocal, '<?php global $probe; return $probe;');
file_put_contents($readGlobal, '<?php global $probe; $actual[] = $probe;');
foreach (['entry', 'include', 'function include', 'GLOBALS', 'extract', 'variable variable'] as $writer) {
    foreach ([false, true] as $preload) {
        unset($probe);
        $readFile = tempnam(sys_get_temp_dir(), 'scope');
        file_put_contents($readFile, $readers);
        if ($preload) {
            $reading = false;
            require $readFile;
        }
        switch ($writer) {
            case 'entry': $probe = 'value'; break;
            case 'include': require $writeGlobal; break;
            case 'function include': writeFromFunction($writeLocal); break;
            case 'GLOBALS': $GLOBALS['probe'] = 'value'; break;
            case 'extract': extract(['probe' => 'value']); break;
            case 'variable variable': $writerName = 'probe'; $$writerName = 'value'; break;
        }
        $reading = true;
        require $readFile;
        $actual[] = $probe;
        $actual[] = extract(['probe' => 'value'], EXTR_IF_EXISTS);
        echo $writer, $preload ? ' / reader first: ' : ' / writer first: ';
        echo $actual === [
            'value', 'value', 'value', 'value', 'value', true,
            'value', 'value', 'value', 'value', 'value', 1,
        ] ? "OK\n" : json_encode($actual) . "\n";
        unlink($readFile);
    }
}
unlink($readLocal);
unlink($readGlobal);
unlink($writeLocal);
unlink($writeGlobal);
var_dump(isset($includeOnly));
--EXPECT--
entry / writer first: OK
entry / reader first: OK
include / writer first: OK
include / reader first: OK
function include / writer first: OK
function include / reader first: OK
GLOBALS / writer first: OK
GLOBALS / reader first: OK
extract / writer first: OK
extract / reader first: OK
variable variable / writer first: OK
variable variable / reader first: OK
bool(false)
