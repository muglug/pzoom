<?php
/** @return non-empty-list */
function foo(string $s) {
    return preg_split("/ /", $s);
}
