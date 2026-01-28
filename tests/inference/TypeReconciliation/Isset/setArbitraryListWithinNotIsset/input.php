<?php
/** @param list<string> $list */
function foo(array &$list, int $offset): void {
    if (!isset($list[$offset])) {
        $list[$offset] = "";
    }
}
