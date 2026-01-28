<?php
/**
 * @template T of array
 * @param T $array
 * @return value-of<T>[]
 */
function getValues($array) {
    return array_values($array);
}
                
