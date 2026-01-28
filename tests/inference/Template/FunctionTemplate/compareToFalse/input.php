<?php
/**
 * @template T as int|false
 * @param T $value
 * @return int
 */
function foo($value) {
    if ($value === false) {
       return -1;
    }
    return $value;
}