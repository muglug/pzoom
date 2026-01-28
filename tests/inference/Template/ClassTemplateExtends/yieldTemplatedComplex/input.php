<?php
/**
 * @template T
 * @psalm-yield T
 */
class a {
}

/**
 * @template TT1
 * @template TT2
 * @extends a<TT2>
 */
class b extends a {}

/** @return Generator<int, b<"test1", "test2">, mixed, "test2"> */
function bb(): \Generator {
    /** @var b<"test1", "test2"> */
    $b = new b;
    $result = yield $b;
    return $result;
}
