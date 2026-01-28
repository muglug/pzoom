<?php
/**
 * @param array{a:array} $array
 * @return array{a:array{b:mixed, ...}, ...}
 * @throw \LogicException
 */
function level3($array) {
    if (!isset($array["a"]["b"])) {
        throw new \LogicException();
    }
    return $array;
}
