<?php
// Run in a dedicated CLI process. See ../README.md for the pinned Composer invocation.
declare(strict_types=1);

use Composer\InstalledVersions;
use Dotenv\Dotenv;
use Symfony\Component\Dotenv\Dotenv as Symfony;

require $argv[1].'/vendor/autoload.php';
$vendor = $argv[1].'/vendor';
foreach (getenv() as $name => $_) {
    putenv((string) $name);
}
putenv('EXTERNAL=outside');
$dir = sys_get_temp_dir().'/phera-dotenv-oracle-'.getmypid();
mkdir($dir);
register_shutdown_function(static function () use ($dir) {
    unlink($dir.'/.env');
    rmdir($dir);
});

// Encode byte strings as hex. A null result is phpdotenv's explicit "unset".
function observe(callable $parse): array {
    $_ENV = $_SERVER = ['EXTERNAL' => 'outside'];
    try {
        $pairs = [];
        foreach ($parse() as $key => $value) {
            $pairs[] = [bin2hex((string) $key), $value === null ? null : bin2hex($value)];
        }
        return ['pairs' => $pairs];
    } catch (Throwable $e) {
        return ['error' => get_class($e), 'message_hex' => bin2hex($e->getMessage())];
    }
}

function snippets(string $vendor): iterable {
    $manual = [
        'comment' => ["", "#comment\n\n # one more", "A=a#suffix", "A=#suffix"],
        'unquoted-spaces' => ["A=hello world  ", "A=one\ttwo  # note"],
        'name' => ["N", "N=old\nN", "N=", "A.B=1", "42=x", "a-b=x",
            "export N", "export 'N' = ok", "é_2=yes", "变量=x", "_=ok", "=secret"],
        'bytes' => ["A=\xff", "A=\x00secret", "\xef\xbb\xbfA=yes\r\n"],
        'quoted' => ["A='line1\r\nline2'\r\n", "A=\"line1\nline2\"",
            "A='missing", "A=\"closed\"tail", "A='\$B # literal'"],
        'expansion' => ['A=$EXTERNAL', 'A=${EXTERNAL}', 'A=$ABSENT', 'A=${ABSENT}',
            "A=\$B\nB=\$A", 'A=${MISSING', 'A=${A:-${B:-deep}}',
            "X=\nA=\${X:-fallback}", "X=yes\nA=\${X:=other}", 'A=${X:=fallback}',
            'A=${EXTERNAL:=other}', 'A=${X:-two words}'],
        'escapes' => ['A="a\"b"', 'A="a\\\\b"', 'A="\\$EXTERNAL"', 'A="\\t\\r\\v\\f"',
            'A="\\q"', 'A="\\n"', "A='\\\\'", "A='\$EXTERNAL'"],
        'escaped-dollar' => ['A=\$EXTERNAL', 'A=\${EXTERNAL}', 'A=\$(date)',
            'A=\\\\$EXTERNAL', 'A=\\\\\\$EXTERNAL'],
        'command' => ['X=$(date)', 'X="$(echo 1)"', "X='\$(date)'"],
    ];
    foreach ($manual as $class => $cases) {
        foreach ($cases as $i => $input) {
            yield ["$class/$i", $input];
        }
    }
    foreach (['', 'export '] as $prefix) {
        foreach (['=', ' = ', "\t=\t"] as $equals) {
            foreach (['', ' # comment', '   ', "\r\n# next"] as $suffix) {
                foreach (['plain'=>'string', 'hash'=>'one#two', 'empty'=>'',
                    'space'=>'two words', 'brace'=>'${BASE}', 'dollar'=>'$BASE',
                    'default'=>'${ABSENT:-yes}', 'escape'=>'a\nb', 'unknown'=>'a\qb'] as $kind=>$v) {
                    foreach (['', "'", '"'] as $q) {
                        $input = "BASE=example\n".$prefix.'KEY'.$equals.$q.$v.$q.$suffix;
                        yield ["matrix-$kind/".bin2hex($prefix.$equals.$q.$suffix), $input];
                    }
                }
            }
        }
    }
    foreach (['vlucas/phpdotenv/tests/fixtures/env', 'symfony/dotenv/Tests/fixtures'] as $source) {
        foreach (scandir("$vendor/$source") as $file) {
            $path = "$vendor/$source/$file";
            if (is_file($path)) {
                yield ["upstream/$source/$file", file_get_contents($path)];
            }
        }
    }
}

$versions = [];
foreach (['vlucas/phpdotenv', 'symfony/dotenv'] as $package) {
    $versions[$package] = InstalledVersions::getPrettyVersion($package);
}
echo json_encode(['versions' => $versions], JSON_UNESCAPED_SLASHES)."\n";
foreach (snippets($vendor) as [$id, $data]) {
    file_put_contents($dir.'/.env', $data);
    $row = [
        'id'=>$id, 'input'=>bin2hex($data),
        'vlucas'=>observe(fn () => Dotenv::createMutable($dir)->load()),
        'symfony'=>observe(fn () => (new Symfony())->parse($data, '.env')),
    ];
    echo json_encode($row, JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR)."\n";
}
