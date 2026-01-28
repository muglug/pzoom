<?php
/**
 * @template T
 */
interface Hasher {
    /**
     * @param T $value
     */
    function hash($value): int;
}

/**
 * @implements Hasher<int>
 */
class IntHasher implements Hasher {
    function hash($value): int {
        return $value % 10;
    }
}

/**
 * @implements Hasher<string>
 */
class StringHasher implements Hasher {
    function hash($value): int {
        return strlen($value);
    }
}