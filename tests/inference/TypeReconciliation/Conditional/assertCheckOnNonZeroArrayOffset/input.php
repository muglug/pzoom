<?php
/**
 * @param array{string,array|null} $a
 * @return string
 */
function f(array $a) {
    assert(is_array($a[1]));
    return $a[0];
}