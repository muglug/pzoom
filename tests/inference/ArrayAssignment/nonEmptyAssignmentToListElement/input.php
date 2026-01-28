<?php
/**
 * @param non-empty-list<string> $arr
 * @return non-empty-list<string>
 */
function takesList(array $arr) : array {
    $arr[0] = "food";

    return $arr;
}
