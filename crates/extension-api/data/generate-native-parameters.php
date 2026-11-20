<?php
// Read the pinned php-src declarations while leaving its checkout untouched.
[$script, $root, $reference] = $argv + [null, null, 'php-8.5.13'];
if ($root === null) {
    throw new RuntimeException("Usage: php $script /path/to/php-src [git-ref]");
}

function git(string ...$args): string {
    global $root;
    $process = proc_open(['git', '-C', $root, ...$args], [1 => ['pipe', 'w']], $pipes);
    $output = stream_get_contents($pipes[1]);
    fclose($pipes[1]);
    if (proc_close($process) !== 0) {
        throw new RuntimeException(implode(' ', $args));
    }
    return $output;
}

function parameters(array $tokens, int &$i): array {
    $params = [];
    $nesting = 0;
    $name = $default = null;
    $reference = $variadic = false;
    for (++$i; isset($tokens[$i]); ++$i) {
        $token = $tokens[$i];
        $text = is_array($token) ? $token[1] : $token;
        if ($nesting === 0) {
            if ($token === ',' || $token === ')') {
                if ($name !== null) {
                    $params[] = [$name, $variadic, $reference, $default === null ? null : trim($default)];
                }
                if ($token === ')') {
                    return $params;
                }
                $name = $default = null;
                $reference = $variadic = false;
                continue;
            }
            if ($name === null && is_array($token)) {
                match ($token[0]) {
                    T_VARIABLE => $name = substr($text, 1),
                    T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG => $reference = true,
                    T_ELLIPSIS => $variadic = true,
                    default => null,
                };
            }
            if ($token === '=') {
                $default = '';
                continue;
            }
        }
        if (in_array($token, ['(', '[', '{'], true) || $text === '#[') {
            ++$nesting;
        } elseif (in_array($token, [')', ']', '}'], true)) {
            --$nesting;
        }
        if ($default !== null) {
            $default .= $text;
        }
    }
    throw new RuntimeException('Unterminated stub signature');
}

function signatures(string $source): array {
    $tokens = token_get_all($source);
    $scope = [];
    $class = $namespace = '';
    $pending = null;
    $result = [];
    foreach ($tokens as $i => $token) {
        if (isset($end) && $i <= $end) {
            continue;
        }
        if ($token === '{') {
            $scope[] = [$class, $namespace];
            if ($pending !== null) {
                [$class, $namespace] = $pending;
                $pending = null;
            }
        } elseif ($token === '}') {
            [$class, $namespace] = array_pop($scope);
        } elseif (is_array($token)) {
            $id = $token[0];
            if ($id === T_NAMESPACE) {
                $next = $i + 1;
                $name = '';
                while (is_array($tokens[$next])) {
                    $name .= trim($tokens[$next++][1]);
                }
                $name = $name === '' ? '' : $name . '\\';
                if ($tokens[$next] === '{') {
                    $pending = ['', $name];
                } else {
                    $namespace = $name;
                }
            } elseif (in_array($id, [T_CLASS, T_INTERFACE, T_TRAIT, T_ENUM], true)
                && ($tokens[$i + 1][0] ?? null) === T_WHITESPACE
                && ($tokens[$i + 2][0] ?? null) === T_STRING) {
                $pending = [$tokens[$i + 2][1], $namespace];
            } elseif ($id === T_FUNCTION) {
                $end = $i + 1;
                $name = '';
                while (is_array($tokens[$end])) {
                    $piece = $tokens[$end++];
                    if ($piece[0] !== T_WHITESPACE && $piece[1] !== '&') {
                        $name .= $piece[1];
                    }
                }
                if ($name !== '' && $tokens[$end] === '(') {
                    $key = $namespace . ($class === '' ? '' : $class . '::') . $name;
                    $result[] = [$key, parameters($tokens, $end), $token[2]];
                }
            }
        }
    }
    return $result;
}

$commit = trim(git('rev-parse', "$reference^{commit}"));
echo "# php-src $reference $commit\n";
foreach (explode("\n", git('ls-tree', '-r', '--name-only', $commit)) as $file) {
    if (!is_runtime_stub($file)) {
        continue;
    }
    foreach (signatures(git('show', "$commit:$file")) as [$name, $parameters, $line]) {
        $namespace = str_contains($name, '\\')
            ? substr($name, 0, strrpos($name, '\\'))
            : '';
        $records[strtolower($name)] = [$parameters, "$file:$line", $namespace];
    }
}
$records = pdo_methods($records);
$records += redis_methods();
ksort($records);

foreach ($records as $name => [$parameters, $location, $namespace]) {
    $required = 0;
    foreach ($parameters as $position => [$_, $variadic, $reference, $expression]) {
        if (!$variadic && $expression === null) {
            $required = $position + 1;
        }
    }
    echo $name, "\t", $location;
    foreach ($parameters as $position => $declaration) {
        [$parameter, $variadic, $reference, $expression] = $declaration;
        $flags = ($position >= $required ? 'o' : 'r') . ($variadic ? 'v' : '-') . ($reference ? '&' : '-');
        echo "\t", $parameter, "\t", $flags, "\t", $expression ?? '', "\t",
            ($declaration[4] ?? default_value($expression, $name, $namespace));
    }
    echo "\n";
}

function pdo_methods(array $records): array {
    foreach ($records as $name => $record) {
        if (str_starts_with($name, 'pdo::')) {
            foreach (['mysql', 'pgsql', 'sqlite'] as $driver) {
                $records['pdo\\' . $driver . substr($name, 3)] ??= $record;
            }
        }
        if (str_starts_with($name, 'pdo_pgsql_ext::')) {
            $method = strstr($name, '::');
            $records['pdo' . $method] = $record;
            $records['pdo\\pgsql' . $method] ??= $record;
            $records['pdo\\mysql' . $method] ??= $record;
        }
    }
    return $records;
}

// Redis is a PECL extension. Its pinned reflection inventory already ships
// with the extension's registration checker.
function redis_methods(): array {
    $file = 'phpredis-6.3.0.json';
    $path = __DIR__ . '/../../extensions/redis/tools/' . $file;
    $metadata = json_decode(file_get_contents($path), true, flags: JSON_THROW_ON_ERROR);
    $records = [];
    foreach ($metadata['methods'] as $name => $method) {
        $parameters = [];
        foreach ($method as $parameter) {
            $default = null;
            if ($parameter['optional'] && !$parameter['variadic']) {
                $value = $parameter['default'];
                $default = is_array($value) ? '[]' : strtolower(var_export($value, true));
            }
            // The PECL inventory stores evaluated defaults, including Redis::MULTI.
            if ($name === 'multi' && $parameter['name'] === 'value') {
                $parameters[] = [$parameter['name'], $parameter['variadic'], $parameter['ref'], 'Redis::MULTI', 'i1'];
            } else {
                $parameters[] = [$parameter['name'], $parameter['variadic'], $parameter['ref'], $default];
            }
        }
        $records['redis::' . strtolower($name)] = [$parameters, $file, ''];
    }
    return $records;
}

function is_runtime_stub(string $file): bool {
    if (!str_ends_with($file, '.stub.php')) {
        return false;
    }
    $parts = explode('/', $file);
    return in_array($parts[0], ['Zend', 'main'], true)
        || ($parts[0] === 'sapi' && $parts[1] === 'cli')
        || ($parts[0] === 'ext' && in_array($parts[1], [
            'bcmath', 'ctype', 'curl', 'date', 'dom', 'fileinfo', 'filter', 'gmp', 'hash', 'iconv', 'intl',
            'json', 'libxml', 'mbstring', 'mysqli', 'openssl', 'pcntl', 'pcre', 'pdo', 'pdo_mysql',
            'pdo_pgsql', 'pdo_sqlite', 'pgsql', 'posix', 'random', 'reflection', 'session', 'sodium',
            'spl', 'sqlite3', 'standard', 'tokenizer', 'zlib',
        ], true));
}

function default_value(?string $expression, string $function, string $namespace): string {
    if ($expression === null || $expression === 'UNKNOWN') {
        return '-';
    }
    try {
        $scope = $namespace === '' ? '' : "namespace $namespace; ";
        // Deprecated declarations remain in the stub catalog, but their
        // diagnostics do not belong in the generated records.
        $value = @eval($scope . "return $expression;");
    } catch (Error $error) {
        // A PHP build can omit constants of an unbuilt function (or an entire
        // extension). Preserve the expression for the inventory checker.
        return '?';
    }
    return match (true) {
        $value === null => 'n',
        $value === [] => 'a',
        $value === true => 't',
        $value === false => 'f',
        is_int($value) => "i$value",
        is_float($value) => 'd' . var_export($value, true),
        is_string($value) => 's' . bin2hex($value),
        $value instanceof UnitEnum => 'e' . get_class($value) . '::' . $value->name,
        default => throw new RuntimeException("Unsupported default $expression in $function"),
    };
}
