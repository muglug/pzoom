<?php
function returnsInt(?int $a, ?int $b): int {
    assert($a !== null || $b !== null);
    return $a ?? $b;
}