<?php
/**
 * @param array<int, array{internal: bool, ported: bool}> $i
 * @return array<int, array{internal: bool, ported: bool}>
 */
function addOneEntry(array $i, int $id): array {
    $i[$id][rand(0, 1) ? "internal" : "ported"] = true;
    return $i;
}
