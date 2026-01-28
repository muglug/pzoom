<?php
/**
 * @template T as Exception
 * @param T::class $type
 * @return T
 * @psalm-suppress UnsafeInstantiation
 */
function a(string $type): Exception {
    return new $type;
}

/**
 * @template T as InvalidArgumentException
 * @param T::class $type
 * @return T
 */
function b(string $type): InvalidArgumentException {
    return a($type);
}