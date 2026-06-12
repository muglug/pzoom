<?php
/**
 * @param non-empty-list<non-empty-string> $parts
 * @return array<string|false>
 */
function recurseGlob2(array $parts): array {
    if (count($parts) < 2) {
        return [];
    }

    $first_dir = $parts[0] . '/';
    $paths = glob($first_dir . '*', GLOB_ONLYDIR | GLOB_NOSORT);
    assert($paths !== false);
    $result = [];
    foreach ($paths as $path) {
        $parts[0] = $path;
        $result = array_merge($result, recurseGlob2($parts));
    }
    array_shift($parts);
    $parts[0] =  $first_dir . $parts[0];

    return array_merge($result, recurseGlob2($parts));
}
