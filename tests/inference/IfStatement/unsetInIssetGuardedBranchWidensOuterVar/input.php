<?php
final class A4 {}
/** @param non-empty-array<string, A4> $b */
function g3(array $b): void {
    if (isset($b['null'])) {
        unset($b['null']);
    }
    if ($b) {
        echo count($b);
    }
}
