<?php
/**
 * @param string|int $key
 */
function get($key, array $arr) : void {
    if (!isset($arr[$key])) {
        if (is_int($key)) {
            $key++;
        }

        if (!isset($arr[$key])) {}
    }
}