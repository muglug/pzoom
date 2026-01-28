<?php
/**
 * @template T
 *
 * @psalm-assert iterable<mixed,T> $i
 *
 * @param iterable<mixed,mixed> $i
 * @param class-string<T> $type
 */
function assertAllInstanceOf(iterable $i, string $type): void {
    /** @psalm-suppress MixedAssignment */
    foreach ($i as $elt) {
        if (!$elt instanceof $type) {
            throw new \UnexpectedValueException("");
        }
    }
}

class A {}

function getIterable(): iterable {
    return [];
}

$iterable = getIterable();
assertAllInstanceOf($iterable, A::class);