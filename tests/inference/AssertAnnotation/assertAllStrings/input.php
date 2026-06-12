<?php
/**
 * @psalm-assert iterable<mixed,string> $i
 *
 * @param iterable<mixed,mixed> $i
 */
function assertAllStrings(iterable $i): void {
    /** @psalm-suppress MixedAssignment */
    foreach ($i as $s) {
        if (!is_string($s)) {
            throw new \UnexpectedValueException("");
        }
    }
}

function getArray(): array {
    return [];
}

function getIterable(): iterable {
    return [];
}

$array = getArray();
assertAllStrings($array);

$iterable = getIterable();
assertAllStrings($iterable);
