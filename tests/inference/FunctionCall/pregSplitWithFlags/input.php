<?php
/** @return list<string> */
function foo(string $s) {
    return preg_split("/ /", $s, -1, PREG_SPLIT_NO_EMPTY | PREG_SPLIT_DELIM_CAPTURE);
}
