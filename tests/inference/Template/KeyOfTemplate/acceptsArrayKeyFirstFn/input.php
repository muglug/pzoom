<?php
/**
 * @template T of array
 * @param T $array
 * @return key-of<T>|null
 */
function getKey($array) {
    return array_key_first($array);
}
                
