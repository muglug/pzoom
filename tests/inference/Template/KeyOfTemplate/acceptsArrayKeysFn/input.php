<?php
/**
 * @template T of array
 * @param T $array
 * @return key-of<T>[]
 */
function getKey($array) {
    return array_keys($array);
}
                
