<?php
/**
 * @param string|int $key
 */
function get($key, array $arr) : void {
    if (!isset($arr[$key])) {
        if (is_string($key)) {
            $key = "p" . $key;
        }

        if (!isset($arr[$key])) {}
    }
}