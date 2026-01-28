<?php
/**
 * @param list<string> $list
 * @return list<string>
 */
function foo(array $list) : array {
    return array_merge($list, ["test"]);
}
