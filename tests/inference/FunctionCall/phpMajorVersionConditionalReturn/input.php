<?php
/**
 * @param non-empty-array<string, int> $assertions
 * @return list<array<string, int>>
 */
function f(array $assertions): array {
    $out = [];
    foreach ($assertions as $var => $v) {
        if ($var[0] === '=') {
            $var = substr($var, 1);
        }
        $out[] = [$var => $v];
    }
    return $out;
}
