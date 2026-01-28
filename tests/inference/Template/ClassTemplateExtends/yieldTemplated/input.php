<?php
/**
 * @template-covariant TValue
 * @psalm-yield TValue
 */
interface Promise {}

/**
 * @template-covariant TValue
 * @template-implements Promise<TValue>
 */
class Success implements Promise {
    /**
     * @psalm-param TValue $value
     */
    public function __construct($value) {}
}

/**
 * @psalm-return Generator<mixed, mixed, mixed, int>
 */
function a(): Generator {
    return b(yield c());
}

function b(string $baz): int {
    return intval($baz);
}

/**
 * @psalm-return Promise<string>
 */
function c(): Promise {
    return new Success("a");
}