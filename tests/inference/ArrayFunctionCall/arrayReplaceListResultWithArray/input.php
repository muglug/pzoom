<?php
/**
 * @param array<int, string> $list
 * @return list<string>
 */
function bar(array $list) : array {
    return array_replace($list, ["test"]);
}
