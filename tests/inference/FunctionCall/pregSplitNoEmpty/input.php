<?php
/** @return non-empty-list */
function foo(string $s) {
    return preg_split("/ /", $s, -1, PREG_SPLIT_NO_EMPTY);
}
