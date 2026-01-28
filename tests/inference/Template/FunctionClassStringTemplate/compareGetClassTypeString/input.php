<?php
/**
 * @template T
 * @param class-string<T> $typeName
 * @param mixed $value
 * @return T
 */
function cast($value, string $typeName) {
    if (is_object($value) && get_class($value) === $typeName) {
        return $value;
    }

    throw new RuntimeException();
}