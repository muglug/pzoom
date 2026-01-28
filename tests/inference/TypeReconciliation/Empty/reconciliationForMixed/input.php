<?php
function foo(array $arr) : void {
    $a = empty($arr["a"]) ? "" : $arr["a"];

    if ($a) {
        if ($a) {}
    }
}
