<?php
/**
 * @template T of array
 * @param T $array
 * @return key-of<T>|null
 */
function getKey(string $key, $array) {
    if (array_key_exists($key, $array)) {
        return $key;
    }
    return null;
}
                
