<?php
/**
 * @param mixed $a
 * @param mixed $b
 * @psalm-assert-if-true string $a
 * @psalm-assert-if-true string $b
 */
function assertAandBAreStrings($a, $b): bool {
    if (!is_string($a)) { return false;}
    if (!is_string($b)) { return false;}

    return true;
}

/**
 * @param mixed $a
 * @param mixed $b
 */
function test($a, $b): string {
    if (!assertAandBAreStrings($a, $b)) {
        throw new \Exception();
    }

    return substr($a, 0, 1) . substr($b, 0, 1);
}
