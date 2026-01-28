<?php
/**
 * @param string[] $strs
 * @return void
 */
function foo(array $strs) {
    foreach ($strs as $str) {
        if (is_string($str)) {}
    }
}