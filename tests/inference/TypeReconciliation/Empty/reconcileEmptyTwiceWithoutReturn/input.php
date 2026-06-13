<?php
function foo(array $arr): void {
    if (!empty($arr["a"])) {
    } else {
        if (empty($arr["dontcare"])) {}
    }

    if (empty($arr["a"])) {}
}
