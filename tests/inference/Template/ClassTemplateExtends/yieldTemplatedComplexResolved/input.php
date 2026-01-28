<?php
/**
 * @template T
 * @psalm-yield T
 */
class a {
}

/**
 * @extends a<"test">
 */
class b extends a {}

/** @return Generator<int, b, mixed, "test"> */
function bb(): \Generator {
    $b = new b;
    $result = yield $b;
    return $result;
}
