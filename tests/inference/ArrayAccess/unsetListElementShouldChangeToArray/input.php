<?php
/**
 * @param list<string> $arr
 * @return list<string>
 */
function takesList(array $arr) : array {
    unset($arr[0]);

    return $arr;
}
