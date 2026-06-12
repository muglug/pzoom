<?php
/**
 * @template T of array
 * @param T $array
 * @return value-of<T>|null
 */
function getValue(string $value, $array) {
    if (in_array($value, $array)) {
        return $value;
    }
    return null;
}
                
